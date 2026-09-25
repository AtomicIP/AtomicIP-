
### batchCompressor baseline — 2026-09-25

> Node v24.21.0 · 200 iterations per scenario · zlib deflate (Node built-in)

| Scenario | Raw (bytes) | Compressed (bytes) | Ratio | Compress (ms/op) | Decompress (ms/op) |
|----------|-------------|-------------------|-------|------------------|--------------------|
| tiny  (  1 swap ) | 244 | 135 | 1.81× | 0.181 | 0.013 |
| small ( 10 swaps) | 2425 | 343 | 7.07× | 0.057 | 0.068 |
| mid   ( 50 swaps) | 12157 | 1187 | 10.24× | 0.101 | 0.046 |
| large (100 swaps) | 24325 | 2239 | 10.86× | 0.158 | 0.137 |
