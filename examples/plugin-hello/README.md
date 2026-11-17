# plugin-hello

Minimal Kadr plugin. It publishes three public sample videos into a virtual library and resolves
them as `direct` playback URLs. Use it as a template; see [`docs/PLUGIN_GUIDE.md`](../../docs/PLUGIN_GUIDE.md).

## Run it externally (local development)

1. Enable plugins in `kadr.toml` and restart Kadr (or turn on **Enable plugins** in the admin UI):

   ```toml
   [plugins]
   enabled = true
   ```

2. Start the plugin. The manifest is public, so it can start before it knows its secret:

   ```bash
   cd examples/plugin-hello && cargo run
   ```

3. As an admin, register and approve it (the registration response contains `secret`, the approval response contains `token`):

   ```bash
   curl -s -X POST localhost:8492/api/v1/admin/plugins \
     -H "Authorization: Bearer $ADMIN_JWT" -H 'Content-Type: application/json' \
     -d '{"base_url":"http://127.0.0.1:9100"}'

   curl -s -X POST localhost:8492/api/v1/admin/plugins/hello/approve \
     -H "Authorization: Bearer $ADMIN_JWT"
   ```

4. Restart the plugin with both values so it can authenticate Kadr and publish its items:

   ```bash
   KADR_PLUGIN_SECRET=<secret> KADR_PLUGIN_TOKEN=<token> cargo run
   ```

The "Hello Videos" library now appears in Kadr, and `GET /api/v1/stream/{id}` redirects to the sample file.

## Package a release archive (for Git installation)

To let admins install `plugin-hello` directly from a GitHub, GitLab, or Gitea release (where Kadr supervises the process and injects `PLUGIN_BIND`, `KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`, and `PLUGIN_DATA_DIR` automatically):

```bash
cargo build --release
cp target/release/plugin-hello plugin
tar -czf plugin-hello-linux-x86_64.tar.gz manifest.json plugin
sha256sum plugin-hello-linux-x86_64.tar.gz > checksums.txt
rm plugin
```

Attach `plugin-hello-linux-x86_64.tar.gz` and `checksums.txt` to a Git release, then paste the repository URL into **Administration → Plugins → Install from Git**.
