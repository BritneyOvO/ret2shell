use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{Context, bail};
use opendal::{Operator, services::Oss};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Clone)]
pub struct AttachmentOss {
  operator: Operator,
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
    Ok(Some(Self {
      operator: Operator::new(builder)?,
    }))
  }

  fn key(game_id: i64, oid: &str) -> String {
    format!("ret2shell/attachments/{game_id}/{oid}")
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
}

fn attachment_oid(record: &[u8]) -> Option<&str> {
  let record = std::str::from_utf8(record).ok()?;
  let (info, path) = record.split_once('\t')?;
  let mut parts = path.split('/');
  if parts.next()? != "challenges" {
    return None;
  }
  parts.next()?;
  if !matches!(parts.next()?, "static" | "mapped") {
    return None;
  }
  let file = parts.next()?;
  if file.starts_with('.') || parts.next().is_some() {
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

  use super::{attachment_oid, file_oid};

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
}
