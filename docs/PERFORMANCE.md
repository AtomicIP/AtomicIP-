
### batchCompressor baseline — 2026-09-04

> Node v24.14.0 · 200 iterations per scenario · zlib deflate (Node built-in)

| Scenario | Raw (bytes) | Compressed (bytes) | Ratio | Compress (ms/op) | Decompress (ms/op) |
|----------|-------------|-------------------|-------|------------------|--------------------|
| tiny  (  1 swap ) | 244 | 135 | 1.81× | 0.025 | 0.013 |
| small ( 10 swaps) | 2425 | 343 | 7.07× | 0.058 | 0.019 |
| mid   ( 50 swaps) | 12157 | 1187 | 10.24× | 0.098 | 0.048 |
| large (100 swaps) | 24325 | 2239 | 10.86× | 0.183 | 0.117 |
