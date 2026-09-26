# Canary Deployment Runbook

The `Canary Deployment` workflow deploys one canary replica first (a
five-percent share of the standard 20-replica pool), checks metrics, and then
increases the canary pool to 25%, 50%, and 100%. The stable and canary
deployments must use the same `app` label selected by the production Service.
Configure the `production` environment with `KUBE_CONFIG` and require an
approval for the workflow when appropriate.

The metrics endpoint passed to the workflow must return JSON in this shape:

```json
{"error_rate": 0.25}
```

`error_rate` is a percentage, so the default one-percent gate is represented
by `1`. The endpoint should aggregate request failures, latency/timeouts, and
availability for the canary during each observation window. Operators should
also review logs, saturation, and business-level indicators before allowing
the final step.

If any metric check fails, the workflow scales the canary to zero and restores
the stable deployment's original replica count. Investigate the release and
metrics before rerunning; do not manually increase traffic around a failed
gate.
