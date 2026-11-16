use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{Context, bail};
use opendal::{Operator, services::Oss};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Clone)]
pub struct AttachmentOss {
  operator: Operator,
  media_enabled: bool,
}

impl AttachmentOss {
  pub fn from_env() -> anyhow::Result<Option<Self>> {
    let Some(bucket) = std::env::var_os("R2S_OSS_BUCKET") else {
      return Ok(None);
    };
    let bucket = bucket
      .into_string()
      .map_err(|_| anyhow::anyhow!("invalid OSS bucket"))?;
    let endpoint = std::env::var("R2S_OSS_ENDPOINT").context("R2S_OSS_ENDPOINT is required")?;
    let mut builder = Oss::default().bucket(&bucket).endpoint(&endpoint);
    if let Ok(endpoint) = std::env::var("R2S_OSS_PUBLIC_ENDPOINT") {
      builder = builder.presign_endpoint(&endpoint);
    }
    let mut oss = Self::new(builder)?;
    oss.media_enabled = match std::env::var("R2S_OSS_MEDIA_ENABLED").as_deref() {
      Ok("true") => true,
      Ok("false") | Err(std::env::VarError::NotPresent) => false,
      _ => bail!("R2S_OSS_MEDIA_ENABLED must be true or false"),
    };
    Ok(Some(oss))
  }

  fn new(builder: Oss) -> opendal::Result<Self> {
    opendal::install_default();
    Ok(Self {
      operator: Operator::new(builder)?,
      media_enabled: false,
    })
  }

  fn key(game_id: i64, oid: &str) -> String {
    format!("ret2shell/attachments/{game_id}/{oid}")
  }

  pub fn media_enabled(&self) -> bool {
    self.media_enabled
  }

  fn media_key(hash: &str) -> anyhow::Result<String> {
    if hash.len() != 64
      || !hash
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
      bail!("invalid media hash");
    }
    Ok(format!("ret2shell/media/{hash}"))
  }

  pub async fn sync_media(&self, media: &r2s_media::Media, hash: &str) -> anyhow::Result<()> {
    let key = Self::media_key(hash)?;
    let mime = media.get_mime_type(hash)?;
    if mime == "image/svg+xml" || self.operator.exists(&key).await? {
      return Ok(());
    }
    let mut file = media.get(hash).await?;
    let mut writer = self
      .operator
      .writer_with(&key)
      .content_type(&mime)
      .chunk(1024 * 1024)
      .await?;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
      let size = file.read(&mut buffer).await?;
      if size == 0 {
        break;
      }
      writer.write(buffer[..size].to_vec()).await?;
    }
    writer.close().await?;
    Ok(())
  }

  pub async fn media_url(&self, hash: &str, mime: &str) -> anyhow::Result<Option<String>> {
    let key = Self::media_key(hash)?;
    if mime == "image/svg+xml" || !self.operator.exists(&key).await? {
      return Ok(None);
    }
    let request = self
      .operator
      .presign_read_with(&key, Duration::from_secs(300))
      .override_content_disposition("inline")
      .await?;
    Ok(Some(request.uri().to_string()))
  }

  pub async fn download_url(
    &self, game_id: i64, oid: &str, file: &str,
  ) -> anyhow::Result<Option<String>> {
    let key = Self::key(game_id, oid);
    if !self.operator.exists(&key).await? {
      return Ok(None);
    }
    let disposition = format!("attachment; filename*=UTF-8''{}", urlencoding::encode(file));
    let request = self
      .operator
      .presign_read_with(&key, Duration::from_secs(300))
      .override_content_disposition(&disposition)
      .override_content_type("application/octet-stream")
      .await?;
    Ok(Some(request.uri().to_string()))
  }

  async fn upload(&self, game_id: i64, repo: &Path, oid: &str) -> anyhow::Result<()> {
    let key = Self::key(game_id, oid);
    if self.operator.exists(&key).await? {
      return Ok(());
    }
    let mut child = Command::new("git")
      .current_dir(repo)
      .args(["cat-file", "blob", oid])
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .kill_on_drop(true)
      .spawn()?;
    let mut stdout = child.stdout.take().context("git stdout unavailable")?;
    let mut writer = self.operator.writer_with(&key).chunk(1024 * 1024).await?;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
      let size = stdout.read(&mut buffer).await?;
      if size == 0 {
        break;
      }
      writer.write(buffer[..size].to_vec()).await?;
    }
    if !child.wait().await?.success() {
      writer.abort().await?;
      bail!("git cat-file failed");
    }
    writer.close().await?;
    Ok(())
  }

  pub async fn sync_repository(&self, game_id: i64, repo: &Path) -> anyhow::Result<()> {
    let output = Command::new("git")
      .current_dir(repo)
      .args(["ls-tree", "-rz", "--full-tree", "HEAD", "--", "challenges"])
      .output()
      .await?;
    if !output.status.success() {
      bail!("git ls-tree failed");
    }
    let mut failures = 0;
    for record in output.stdout.split(|b| *b == 0).filter(|r| !r.is_empty()) {
      if let Some(oid) = attachment_oid(record)
        && let Err(err) = self.upload(game_id, repo, oid).await
      {
        tracing::warn!(game_id, oid, error=?err, "failed to sync attachment");
        failures += 1;
      }
    }
    if failures > 0 {
      bail!("{failures} attachments failed to sync");
    }
    Ok(())
  }

  pub async fn sync_changes(
    &self, game_id: i64, repo: &Path, old: &str, new: &str,
  ) -> anyhow::Result<()> {
    for oid in changed_attachment_oids(repo, old, new).await? {
      self.upload(game_id, repo, &oid).await?;
    }
    Ok(())
  }
}

async fn changed_attachment_oids(repo: &Path, old: &str, new: &str) -> anyhow::Result<Vec<String>> {
  let output = Command::new("git")
    .current_dir(repo)
    .args([
      "diff",
      "--name-only",
      "-z",
      "--diff-filter=ACMRT",
      old,
      new,
      "--",
      "challenges",
    ])
    .output()
    .await?;
  if !output.status.success() {
    bail!("git diff failed");
  }
  let mut oids = Vec::new();
  for path in output
    .stdout
    .split(|byte| *byte == 0)
    .filter(|path| !path.is_empty())
  {
    let path = std::str::from_utf8(path).context("invalid attachment path")?;
    if !is_attachment_path(path) {
      continue;
    }
    let literal = format!(":(literal){path}");
    let tree = Command::new("git")
      .current_dir(repo)
      .args(["ls-tree", "-rz", "--full-tree", new, "--", &literal])
      .output()
      .await?;
    if !tree.status.success() {
      bail!("git ls-tree failed");
    }
    for record in tree
      .stdout
      .split(|byte| *byte == 0)
      .filter(|record| !record.is_empty())
    {
      if let Some(oid) = attachment_oid(record) {
        oids.push(oid.to_owned());
      }
    }
  }
  Ok(oids)
}

fn is_attachment_path(path: &str) -> bool {
  let mut parts = path.split('/');
  matches!(parts.next(), Some("challenges"))
    && parts.next().is_some_and(|name| !name.is_empty())
    && matches!(parts.next(), Some("static" | "mapped"))
    && parts
      .next()
      .is_some_and(|file| !file.is_empty() && !file.starts_with('.'))
    && parts.next().is_none()
}

fn attachment_oid(record: &[u8]) -> Option<&str> {
  let record = std::str::from_utf8(record).ok()?;
  let (info, path) = record.split_once('\t')?;
  if !is_attachment_path(path) {
    return None;
  }
  let (mode, oid) = info.split_once(" blob ")?;
  if !matches!(mode, "100644" | "100755") || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
    return None;
  }
  Some(oid)
}

pub async fn file_oid(
  repo: &Path, challenge: &str, folder: &str, file: &str,
) -> anyhow::Result<String> {
  let path = format!(":(literal)challenges/{challenge}/{folder}/{file}");
  let output = Command::new("git")
    .current_dir(repo)
    .args(["ls-tree", "-rz", "--full-tree", "HEAD", "--", &path])
    .output()
    .await?;
  if !output.status.success() {
    bail!("failed to read committed attachment");
  }
  let record = output
    .stdout
    .split(|b| *b == 0)
    .next()
    .context("attachment is not committed")?;
  Ok(
    attachment_oid(record)
      .context("attachment is not a regular committed file")?
      .to_owned(),
  )
}

#[cfg(test)]
mod tests {
  use std::{fs, process::Command, time::SystemTime};

  use opendal::services::Oss;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};

  use super::{AttachmentOss, attachment_oid, changed_attachment_oids, file_oid};

  #[tokio::test]
  async fn media_sync_uploads_image_bytes_and_content_type() {
    const PNG: &[u8] = &[
      137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
      0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 252, 255, 31, 0, 3, 3, 2,
      0, 239, 191, 167, 219, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let suffix = SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    let dir = std::env::temp_dir().join(format!("r2s-media-oss-{}-{suffix}", std::process::id()));
    let media = r2s_media::Media::try_open(&dir).await.unwrap();
    let model = media.save(PNG).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let oss = AttachmentOss::new(
      Oss::default()
        .bucket("test-bucket")
        .endpoint(&endpoint)
        .addressing_style("path")
        .access_key_id("test-key")
        .access_key_secret("test-secret"),
    )
    .unwrap();
    let server = tokio::spawn(async move {
      let (mut head, _) = listener.accept().await.unwrap();
      let mut buf = [0; 8192];
      assert!(head.read(&mut buf).await.unwrap() > 0);
      head
        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
      let (mut put, _) = listener.accept().await.unwrap();
      let mut request = Vec::new();
      loop {
        let n = put.read(&mut buf).await.unwrap();
        assert!(n > 0);
        request.extend_from_slice(&buf[..n]);
        if let Some(pos) = request.windows(4).position(|window| window == b"\r\n\r\n")
          && request.len() >= pos + 4 + PNG.len()
        {
          break;
        }
      }
      let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
      assert!(text.starts_with("put "));
      assert!(text.contains("content-type: image/png"));
      assert!(request.ends_with(PNG));
      put
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    });
    oss.sync_media(&media, &model.hash).await.unwrap();
    server.await.unwrap();
    fs::remove_dir_all(dir).unwrap();
  }

  #[tokio::test]
  async fn media_redirect_requires_an_existing_non_svg_object() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let oss = AttachmentOss::new(
      Oss::default()
        .bucket("test-bucket")
        .endpoint(&endpoint)
        .addressing_style("path")
        .access_key_id("test-key")
        .access_key_secret("test-secret"),
    )
    .unwrap();
    let hash = "a".repeat(64);
    assert!(
      oss
        .media_url(&hash, "image/svg+xml")
        .await
        .unwrap()
        .is_none()
    );
    assert!(oss.media_url("invalid", "image/png").await.is_err());
    let server = tokio::spawn(async move {
      for status in ["404 Not Found", "200 OK"] {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let size = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..size]);
        assert!(request.starts_with("HEAD "));
        assert!(request.contains("/ret2shell/media/"));
        stream
          .write_all(
            format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
              .as_bytes(),
          )
          .await
          .unwrap();
      }
    });
    assert!(oss.media_url(&hash, "image/png").await.unwrap().is_none());
    let url = oss.media_url(&hash, "image/png").await.unwrap().unwrap();
    assert!(url.contains("ret2shell/media/"));
    assert!(url.contains("response-content-disposition="));
    server.await.unwrap();
  }

  #[tokio::test]
  async fn changed_blobs_include_renames_and_exclude_deleted_and_other_files() {
    let suffix = SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    let repo = std::env::temp_dir().join(format!("r2s-oss-diff-{}-{suffix}", std::process::id()));
    let static_dir = repo.join("challenges/test/static");
    let checker = repo.join("challenges/test/checker");
    fs::create_dir_all(&static_dir).unwrap();
    fs::create_dir_all(&checker).unwrap();
    fs::write(static_dir.join("old"), b"renamed").unwrap();
    fs::write(static_dir.join("removed"), b"removed").unwrap();
    let git = |args: &[&str]| {
      let output = Command::new("git")
        .current_dir(&repo)
        .args(args)
        .output()
        .unwrap();
      assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
      );
      String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-qm",
      "first",
    ]);
    let old = git(&["rev-parse", "HEAD"]);
    fs::rename(static_dir.join("old"), static_dir.join("[new]")).unwrap();
    fs::remove_file(static_dir.join("removed")).unwrap();
    fs::write(static_dir.join("added"), b"added").unwrap();
    fs::write(checker.join("checker"), b"checker").unwrap();
    git(&["add", "-A"]);
    git(&[
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-qm",
      "second",
    ]);
    let new = git(&["rev-parse", "HEAD"]);
    let mut actual = changed_attachment_oids(&repo, &old, &new).await.unwrap();
    actual.sort();
    let mut expected = vec![
      git(&["rev-parse", "HEAD:challenges/test/static/[new]"]),
      git(&["rev-parse", "HEAD:challenges/test/static/added"]),
    ];
    expected.sort();
    assert_eq!(actual, expected);
    fs::remove_dir_all(repo).unwrap();
  }

  #[test]
  fn only_selected_attachment_blobs_are_synced() {
    assert_eq!(
      attachment_oid(b"100644 blob abcd\tchallenges/test/static/file.zip"),
      Some("abcd")
    );
    assert_eq!(
      attachment_oid(b"100644 blob abcd\tchallenges/test/mapped/file.zip"),
      Some("abcd")
    );
    assert_eq!(
      attachment_oid(b"100644 blob abcd\tchallenges/test/checker/file"),
      None
    );
    assert_eq!(
      attachment_oid(b"120000 blob abcd\tchallenges/test/static/link"),
      None
    );
    assert_eq!(
      attachment_oid(b"100644 blob abcd\tchallenges/test/static/.hidden"),
      None
    );
  }

  #[tokio::test]
  async fn oss_requests_use_the_installed_http_transport() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let mut request = [0; 4096];
      assert!(stream.read(&mut request).await.unwrap() > 0);
      stream
        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    });
    let oss = AttachmentOss::new(
      Oss::default()
        .bucket("test-bucket")
        .endpoint(&endpoint)
        .addressing_style("path")
        .access_key_id("test-key")
        .access_key_secret("test-secret"),
    )
    .unwrap();
    assert!(!oss.operator.exists("attachment").await.unwrap());
    server.await.unwrap();
  }

  #[tokio::test]
  async fn committed_file_lookup_treats_filename_as_literal() {
    let suffix = SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    let repo = std::env::temp_dir().join(format!("r2s-oss-{}-{suffix}", std::process::id()));
    let folder = repo.join("challenges/test/static");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("[sample].txt"), b"attachment").unwrap();
    let git = |args: &[&str]| {
      let status = Command::new("git")
        .current_dir(&repo)
        .args(args)
        .status()
        .unwrap();
      assert!(status.success());
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-qm",
      "test",
    ]);

    let oid = file_oid(&repo, "test", "static", "[sample].txt")
      .await
      .unwrap();
    assert_eq!(oid.len(), 40);
    assert!(
      file_oid(&repo, "test", "static", "sample.txt")
        .await
        .is_err()
    );
    fs::remove_dir_all(repo).unwrap();
  }

  #[tokio::test]
  #[ignore = "requires a private OSS test bucket with object deletion permission"]
  async fn sync_and_download_from_oss() {
    let oss = AttachmentOss::from_env()
      .unwrap()
      .expect("OSS must be configured");
    let repo = std::env::temp_dir().join(format!("r2s-oss-integration-{}", std::process::id()));
    let folder = repo.join("challenges/test/static");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
      folder.join("attachment.txt"),
      b"ret2shell oss integration test\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
      assert!(
        Command::new("git")
          .current_dir(&repo)
          .args(args)
          .status()
          .unwrap()
          .success()
      );
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-qm",
      "test",
    ]);

    let oid = file_oid(&repo, "test", "static", "attachment.txt")
      .await
      .unwrap();
    let game_id = i64::MAX - i64::from(std::process::id());
    oss.sync_repository(game_id, &repo).await.unwrap();
    let url = oss
      .download_url(game_id, &oid, "attachment.txt")
      .await
      .unwrap()
      .unwrap();
    let response = Command::new("curl")
      .args(["--fail", "--silent", "--max-time", "30", &url])
      .output()
      .unwrap();
    assert!(response.status.success(), "signed OSS download failed");
    assert_eq!(response.stdout, b"ret2shell oss integration test\n");
    oss
      .operator
      .delete(&AttachmentOss::key(game_id, &oid))
      .await
      .unwrap();
    fs::remove_dir_all(repo).unwrap();
  }
}
