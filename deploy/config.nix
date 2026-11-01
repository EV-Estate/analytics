# Prod config (secret-free, committable). Evaluated to JSON at image-build time
# by the flake — the runtime container carries no `nix`, only the baked result.
# The two secrets resolve at startup from the container env that gitops' k8s
# Secret injects (`envFrom` `kubernetes-analytics`); `.env` is the config
# primitive's indirection, and a missing var fails the boot loudly. The DB on
# the `/data` PVC is 100% of state; litestream (gitops sidecar) replicates it
# to R2 and restores it onto fresh PVCs.
{
  socket_addr = "0.0.0.0:59079";
  db_path = "/data/app.db";
  admins = [ ];
  maps_api_key.env = "GOOGLE_MAPS_KEY";
  admin_token.env = "ANALYTICS_ADMIN_TOKEN";
}
