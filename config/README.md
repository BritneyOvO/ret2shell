# Configuration Files

## `config.toml` (required)

> provided as [config.sample.toml](config.sample.toml) at this directory.

The example configuration file for ret2shell.

## `sensitive_word_list.txt`

> Path of this file could be changed in `config.toml`.

Words in this file would trigger the sensitive word detection mechanism in the server.

## OSS Attachment Downloads

Optional environment variables for the server process:

- `R2S_OSS_BUCKET`: private OSS bucket name. Leave unset to disable OSS.
- `R2S_OSS_ENDPOINT`: OSS endpoint used by the server, for example `https://oss-cn-hangzhou.aliyuncs.com`.
- `R2S_OSS_PUBLIC_ENDPOINT`: optional public endpoint used in browser download URLs when the server endpoint is internal.
- `ALIBABA_CLOUD_ACCESS_KEY_ID` and `ALIBABA_CLOUD_ACCESS_KEY_SECRET`: credentials with access to the attachment prefix.
- `ALIBABA_CLOUD_SECURITY_TOKEN`: security token when using temporary credentials.
- `R2S_OSS_MEDIA_ENABLED`: set to `true` to also serve supported media images through OSS; defaults to `false`.

The server queues changed static and mapped challenge files for upload after an attachment upload or Git push. Failed jobs are retried, and a full reconciliation runs at startup and every 24 hours to recover missed jobs and existing files. Git blob versions are stored under `ret2shell/attachments/`. Grant the server permission to read and write this prefix and keep the bucket private. Existing downloads fall back to the application server while a file is not yet synced or OSS is unavailable. Browser download links expire after five minutes. Checker attachments remain local.

For Helm deployments, set `platform.attachmentOss.bucket`, `endpoint`, and optionally `publicEndpoint`. Set `credentialsSecret` to an existing Kubernetes Secret containing the Alibaba Cloud credential environment variables above, or use an available workload identity.

To enable media, also set `platform.attachmentOss.mediaEnabled: true` and grant the credentials read and write access to `ret2shell/media/`. Image uploads are queued for sync, and database records are reconciled at startup and every 24 hours. Original files remain local; SVG images always stay on the application server so the existing content security policy applies. Validated non-SVG media requests redirect to a five-minute signed OSS URL when available, with a local fallback if the object is missing or URL generation fails. A signed URL can be shared until it expires, and failures after the browser follows the redirect cannot fall back automatically. Media objects are not automatically deleted from OSS.

Objects from older file versions are not automatically deleted. Configure an OSS lifecycle rule appropriate to the bucket; active objects removed by the rule are uploaded again on a subsequent scan. Lifecycle expiration must allow for the five-minute download URL lifetime and any downloads already in progress. OSS storage, requests, and outbound traffic incur charges.
