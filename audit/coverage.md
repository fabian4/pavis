| file                                                     | coverage | covered   | missed_lines                                                 |
|----------------------------------------------------------|----------|-----------|--------------------------------------------------------------|
| crates/pavctl/src/commands.rs                            | 100.00%  | 4 / 4     |                                                              |
| crates/pavctl/src/commands/check.rs                      | 100.00%  | 9 / 9     |                                                              |
| crates/pavctl/src/commands/convert.rs                    | 100.00%  | 19 / 19   |                                                              |
| crates/pavctl/src/commands/gen.rs                        | 100.00%  | 14 / 14   |                                                              |
| crates/pavctl/src/commands/view.rs                       | 100.00%  | 13 / 13   |                                                              |
| crates/pavctl/src/format.rs                              | 97.30%   | 72 / 74   | 30, 51                                                       |
| crates/pavctl/src/main.rs                                | 100.00%  | 26 / 26   |                                                              |
| crates/pavctl/src/parse.rs                               | 100.00%  | 9 / 9     |                                                              |
| crates/pavis-codec-api/src/lib.rs                        | 91.30%   | 21 / 23   | 104, 118                                                     |
| crates/pavis-codec-serde/src/config/convert.rs           | 100.00%  | 17 / 17   |                                                              |
| crates/pavis-codec-serde/src/config/convert/routes.rs    | 76.30%   | 132 / 173 | 325, 343-361, 400-404, 493, 511, 515, 523, 549-565, 577, 583 |
| crates/pavis-codec-serde/src/config/convert/server.rs    | 100.00%  | 37 / 37   |                                                              |
| crates/pavis-codec-serde/src/config/convert/telemetry.rs | 96.15%   | 50 / 52   | 216, 220                                                     |
| crates/pavis-codec-serde/src/config/convert/upstreams.rs | 95.28%   | 121 / 127 | 280, 346, 348, 351, 385, 394                                 |
| crates/pavis-codec-serde/src/config/types.rs             | 100.00%  | 16 / 16   |                                                              |
| crates/pavis-codec-serde/src/config/validation.rs        | 90.62%   | 29 / 32   | 72, 74-75                                                    |
| crates/pavis-codec-serde/src/lib.rs                      | 92.68%   | 38 / 41   | 61, 64, 97                                                   |
| crates/pavis-codec-serde/src/serde_helpers.rs            | 100.00%  | 10 / 10   |                                                              |
| crates/pavis-core/src/runtime.rs                         | 100.00%  | 8 / 8     |                                                              |
| crates/pavis-core/src/serde_impl.rs                      | 95.24%   | 20 / 21   | 26                                                           |
| crates/pavis-core/src/validate.rs                        | 100.00%  | 6 / 6     |                                                              |
| crates/pavis-core/src/validate/headers.rs                | 93.33%   | 28 / 30   | 38, 41                                                       |
| crates/pavis-core/src/validate/routes.rs                 | 95.56%   | 43 / 45   | 88-89                                                        |
| crates/pavis-core/src/validate/server.rs                 | 100.00%  | 7 / 7     |                                                              |
| crates/pavis-core/src/validate/upstreams.rs              | 100.00%  | 9 / 9     |                                                              |
| crates/pavis-ingest-api/src/lib.rs                       | 100.00%  | 13 / 13   |                                                              |
| crates/pavis-ingest-file/src/lib.rs                      | 100.00%  | 23 / 23   |                                                              |
| crates/pavis-ingest-file/src/watch.rs                    | 86.36%   | 57 / 66   | 45, 69-71, 85, 104-105, 111-112                              |
| crates/pavis-pvs/src/header.rs                           | 100.00%  | 17 / 17   |                                                              |
| crates/pavis-pvs/src/read.rs                             | 100.00%  | 21 / 21   |                                                              |
| crates/pavis-pvs/src/verify.rs                           | 90.67%   | 68 / 75   | 98-104                                                       |
| crates/pavis-pvs/src/write.rs                            | 100.00%  | 21 / 21   |                                                              |
| crates/pavis-relay/src/app.rs                            | 100.00%  | 45 / 45   |                                                              |
| crates/pavis-relay/src/codec.rs                          | 100.00%  | 4 / 4     |                                                              |
| crates/pavis-relay/src/config/env.rs                     | 100.00%  | 27 / 27   |                                                              |
| crates/pavis-relay/src/config/load.rs                    | 100.00%  | 31 / 31   |                                                              |
| crates/pavis-relay/src/config/types.rs                   | 100.00%  | 57 / 57   |                                                              |
| crates/pavis-relay/src/handlers.rs                       | 97.08%   | 166 / 171 | 158, 168-171                                                 |
| crates/pavis-relay/src/ingest.rs                         | 100.00%  | 7 / 7     |                                                              |
| crates/pavis-relay/src/main.rs                           | 0.00%    | 0 / 5     | 14-18                                                        |
| crates/pavis-relay/src/pipeline.rs                       | 84.95%   | 79 / 93   | 17, 22, 30, 80-86, 102-106, 118-121, 136, 184                |
| crates/pavis-relay/src/routes.rs                         | 94.74%   | 18 / 19   | 32                                                           |
| crates/pavis-relay/src/state.rs                          | 97.57%   | 201 / 206 | 236-237, 355-356, 432                                        |
| crates/pavis/src/agent/backoff.rs                        | 100.00%  | 7 / 7     |                                                              |
| crates/pavis/src/agent/lkg.rs                            | 100.00%  | 30 / 30   |                                                              |
| crates/pavis/src/agent/worker.rs                         | 93.22%   | 55 / 59   | 122, 144-146                                                 |
| crates/pavis/src/load.rs                                 | 100.00%  | 8 / 8     |                                                              |
| crates/pavis/src/main.rs                                 | 72.15%   | 57 / 79   | 111-112, 117, 124, 131-134, 153-162, 182-186, 194, 202-203   |
| crates/pavis/src/proxy/header_ops.rs                     | 78.85%   | 82 / 104  | 48-49, 75, 89, 128-140, 147-151, 173, 186, 197-198           |
| crates/pavis/src/proxy/service.rs                        | 81.25%   | 39 / 48   | 54-60, 67-68, 72-74, 288                                     |
| crates/pavis/src/router.rs                               | 97.56%   | 40 / 41   | 89                                                           |
| crates/pavis/src/router/matcher.rs                       | 88.89%   | 32 / 36   | 20, 67-68, 72                                                |
| crates/pavis/src/state.rs                                | 100.00%  | 15 / 15   |                                                              |
| crates/pavis/src/telemetry.rs                            | 100.00%  | 5 / 5     |                                                              |
| crates/pavis/src/telemetry/access_log.rs                 | 100.00%  | 35 / 35   |                                                              |
| crates/pavis/src/upstream.rs                             | 100.00%  | 8 / 8     |                                                              |
| crates/pavis/src/upstream/cluster.rs                     | 100.00%  | 27 / 27   |                                                              |
| crates/pavis/src/upstream/load_balance.rs                | 94.44%   | 17 / 18   | 38                                                           |
| crates/pavis/src/upstream/resolver.rs                    | 35.11%   | 33 / 94   | 32-42, 48-57, 94-95, 99-111, 129, 134-161, 168-187, 198-203  |

Total coverage: 89.99%
