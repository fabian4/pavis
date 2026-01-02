| file                                                     | coverage | covered   | missed_lines                                  |
|----------------------------------------------------------|----------|-----------|-----------------------------------------------|
| crates/pavctl/src/commands.rs                            | 100.00%  | 4 / 4     |                                               |
| crates/pavctl/src/commands/check.rs                      | 100.00%  | 9 / 9     |                                               |
| crates/pavctl/src/commands/convert.rs                    | 100.00%  | 19 / 19   |                                               |
| crates/pavctl/src/commands/gen.rs                        | 100.00%  | 14 / 14   |                                               |
| crates/pavctl/src/commands/view.rs                       | 100.00%  | 13 / 13   |                                               |
| crates/pavctl/src/format.rs                              | 100.00%  | 67 / 67   |                                               |
| crates/pavctl/src/main.rs                                | 84.62%   | 22 / 26   | 62-65                                         |
| crates/pavctl/src/parse.rs                               | 100.00%  | 9 / 9     |                                               |
| crates/pavis-codec-api/src/lib.rs                        | 100.00%  | 10 / 10   |                                               |
| crates/pavis-codec-serde/src/config/convert.rs           | 100.00%  | 12 / 12   |                                               |
| crates/pavis-codec-serde/src/config/convert/routes.rs    | 100.00%  | 81 / 81   |                                               |
| crates/pavis-codec-serde/src/config/convert/server.rs    | 100.00%  | 12 / 12   |                                               |
| crates/pavis-codec-serde/src/config/convert/telemetry.rs | 100.00%  | 29 / 29   |                                               |
| crates/pavis-codec-serde/src/config/convert/upstreams.rs | 100.00%  | 52 / 52   |                                               |
| crates/pavis-codec-serde/src/config/types.rs             | 100.00%  | 10 / 10   |                                               |
| crates/pavis-codec-serde/src/config/types/upstreams.rs   | 100.00%  | 9 / 9     |                                               |
| crates/pavis-codec-serde/src/config/validation.rs        | 100.00%  | 20 / 20   |                                               |
| crates/pavis-codec-serde/src/lib.rs                      | 96.15%   | 25 / 26   | 20                                            |
| crates/pavis-codec-serde/src/serde_helpers.rs            | 100.00%  | 10 / 10   |                                               |
| crates/pavis-core/src/runtime.rs                         | 100.00%  | 8 / 8     |                                               |
| crates/pavis-core/src/serde_impl.rs                      | 95.24%   | 20 / 21   | 26                                            |
| crates/pavis-core/src/validate.rs                        | 100.00%  | 5 / 5     |                                               |
| crates/pavis-core/src/validate/headers.rs                | 100.00%  | 19 / 19   |                                               |
| crates/pavis-core/src/validate/routes.rs                 | 100.00%  | 44 / 44   |                                               |
| crates/pavis-core/src/validate/server.rs                 | 100.00%  | 6 / 6     |                                               |
| crates/pavis-core/src/validate/upstreams.rs              | 100.00%  | 11 / 11   |                                               |
| crates/pavis-ingest-api/src/lib.rs                       | 100.00%  | 13 / 13   |                                               |
| crates/pavis-ingest-file/src/lib.rs                      | 100.00%  | 23 / 23   |                                               |
| crates/pavis-ingest-file/src/watch.rs                    | 86.36%   | 57 / 66   | 45, 69-71, 85, 104-105, 111-112               |
| crates/pavis-pvs/src/header.rs                           | 100.00%  | 17 / 17   |                                               |
| crates/pavis-pvs/src/read.rs                             | 100.00%  | 21 / 21   |                                               |
| crates/pavis-pvs/src/verify.rs                           | 100.00%  | 67 / 67   |                                               |
| crates/pavis-pvs/src/write.rs                            | 100.00%  | 21 / 21   |                                               |
| crates/pavis-relay/src/app.rs                            | 100.00%  | 45 / 45   |                                               |
| crates/pavis-relay/src/codec.rs                          | 100.00%  | 4 / 4     |                                               |
| crates/pavis-relay/src/config/env.rs                     | 100.00%  | 27 / 27   |                                               |
| crates/pavis-relay/src/config/load.rs                    | 100.00%  | 31 / 31   |                                               |
| crates/pavis-relay/src/config/types.rs                   | 100.00%  | 57 / 57   |                                               |
| crates/pavis-relay/src/handlers.rs                       | 97.08%   | 166 / 171 | 158, 168-171                                  |
| crates/pavis-relay/src/ingest.rs                         | 100.00%  | 7 / 7     |                                               |
| crates/pavis-relay/src/main.rs                           | 0.00%    | 0 / 5     | 14-18                                         |
| crates/pavis-relay/src/pipeline.rs                       | 84.95%   | 79 / 93   | 17, 22, 30, 80-86, 102-106, 116-119, 132, 180 |
| crates/pavis-relay/src/routes.rs                         | 94.74%   | 18 / 19   | 32                                            |
| crates/pavis-relay/src/state.rs                          | 97.09%   | 200 / 206 | 236-237, 355-356, 368, 432                    |
| crates/pavis/src/agent/backoff.rs                        | 100.00%  | 7 / 7     |                                               |
| crates/pavis/src/agent/lkg.rs                            | 100.00%  | 31 / 31   |                                               |
| crates/pavis/src/agent/worker.rs                         | 93.22%   | 55 / 59   | 122, 144-146                                  |
| crates/pavis/src/load.rs                                 | 100.00%  | 7 / 7     |                                               |
| crates/pavis/src/main.rs                                 | 73.61%   | 53 / 72   | 113-114, 118-121, 140-149, 166-175            |
| crates/pavis/src/proxy/header_ops.rs                     | 100.00%  | 28 / 28   |                                               |
| crates/pavis/src/proxy/service.rs                        | 100.00%  | 12 / 12   |                                               |
| crates/pavis/src/router.rs                               | 100.00%  | 17 / 17   |                                               |
| crates/pavis/src/router/matcher.rs                       | 100.00%  | 37 / 37   |                                               |
| crates/pavis/src/state.rs                                | 100.00%  | 15 / 15   |                                               |
| crates/pavis/src/telemetry.rs                            | 100.00%  | 5 / 5     |                                               |
| crates/pavis/src/telemetry/access_log.rs                 | 100.00%  | 35 / 35   |                                               |
| crates/pavis/src/upstream.rs                             | 100.00%  | 6 / 6     |                                               |
| crates/pavis/src/upstream/cluster.rs                     | 100.00%  | 11 / 11   |                                               |
| crates/pavis/src/upstream/load_balance.rs                | 100.00%  | 20 / 20   |                                               |

Total coverage: 96.17%
