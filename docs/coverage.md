| file                                                     | coverage | covered   | missed_lines                                                                    |
|----------------------------------------------------------|----------|-----------|---------------------------------------------------------------------------------|
| crates/pavctl/src/commands.rs                            | 100.00%  | 4 / 4     |                                                                                 |
| crates/pavctl/src/commands/check.rs                      | 100.00%  | 9 / 9     |                                                                                 |
| crates/pavctl/src/commands/convert.rs                    | 100.00%  | 18 / 18   |                                                                                 |
| crates/pavctl/src/commands/gen.rs                        | 100.00%  | 24 / 24   |                                                                                 |
| crates/pavctl/src/commands/view.rs                       | 100.00%  | 13 / 13   |                                                                                 |
| crates/pavctl/src/format.rs                              | 92.05%   | 81 / 88   | 31, 37, 54, 68, 88-89, 110                                                      |
| crates/pavctl/src/main.rs                                | 100.00%  | 26 / 26   |                                                                                 |
| crates/pavctl/src/parse.rs                               | 100.00%  | 9 / 9     |                                                                                 |
| crates/pavis-benchkit/src/bin/bench-upstream.rs          | 68.49%   | 100 / 146 | 29-74, 190-195                                                                  |
| crates/pavis-benchkit/src/metrics.rs                     | 100.00%  | 6 / 6     |                                                                                 |
| crates/pavis-codec-api/src/lib.rs                        | 100.00%  | 21 / 21   |                                                                                 |
| crates/pavis-codec-serde/src/config/convert.rs           | 100.00%  | 25 / 25   |                                                                                 |
| crates/pavis-codec-serde/src/config/convert/routes.rs    | 88.18%   | 179 / 203 | 69, 96, 102, 185, 195, 209, 218, 226-237, 248, 261, 294-295, 370, 381, 384, 387 |
| crates/pavis-codec-serde/src/config/convert/server.rs    | 92.45%   | 49 / 53   | 35, 65, 84, 94                                                                  |
| crates/pavis-codec-serde/src/config/convert/telemetry.rs | 94.12%   | 48 / 51   | 102, 106-108                                                                    |
| crates/pavis-codec-serde/src/config/convert/upstreams.rs | 90.68%   | 146 / 161 | 39, 60-63, 133, 156, 171, 180, 195, 207, 248, 257, 265, 273                     |
| crates/pavis-codec-serde/src/config/types.rs             | 100.00%  | 11 / 11   |                                                                                 |
| crates/pavis-codec-serde/src/config/validation.rs        | 90.32%   | 28 / 31   | 69, 71-72                                                                       |
| crates/pavis-codec-serde/src/lib.rs                      | 95.00%   | 19 / 20   | 60                                                                              |
| crates/pavis-codec-serde/src/serde_helpers.rs            | 100.00%  | 10 / 10   |                                                                                 |
| crates/pavis-core/src/runtime.rs                         | 100.00%  | 9 / 9     |                                                                                 |
| crates/pavis-core/src/serde_impl.rs                      | 95.24%   | 20 / 21   | 26                                                                              |
| crates/pavis-core/src/validate.rs                        | 100.00%  | 15 / 15   |                                                                                 |
| crates/pavis-core/src/validate/headers.rs                | 93.33%   | 28 / 30   | 38, 41                                                                          |
| crates/pavis-core/src/validate/routes.rs                 | 94.64%   | 53 / 56   | 100, 108-109                                                                    |
| crates/pavis-core/src/validate/server.rs                 | 100.00%  | 14 / 14   |                                                                                 |
| crates/pavis-core/src/validate/upstreams.rs              | 100.00%  | 9 / 9     |                                                                                 |
| crates/pavis-ingest-api/src/lib.rs                       | 100.00%  | 13 / 13   |                                                                                 |
| crates/pavis-ingest-file/src/lib.rs                      | 100.00%  | 42 / 42   |                                                                                 |
| crates/pavis-ingest-file/src/watch.rs                    | 85.33%   | 64 / 75   | 45, 69-71, 85, 96-97, 105-110, 123-124                                          |
| crates/pavis-pvs/src/header.rs                           | 100.00%  | 17 / 17   |                                                                                 |
| crates/pavis-pvs/src/read.rs                             | 100.00%  | 31 / 31   |                                                                                 |
| crates/pavis-pvs/src/verify.rs                           | 96.33%   | 105 / 109 | 87, 97-99                                                                       |
| crates/pavis-pvs/src/write.rs                            | 100.00%  | 21 / 21   |                                                                                 |
| crates/pavis-relay/src/app.rs                            | 100.00%  | 56 / 56   |                                                                                 |
| crates/pavis-relay/src/config/env.rs                     | 100.00%  | 27 / 27   |                                                                                 |
| crates/pavis-relay/src/config/load.rs                    | 100.00%  | 31 / 31   |                                                                                 |
| crates/pavis-relay/src/config/types.rs                   | 92.98%   | 53 / 57   | 114-120                                                                         |
| crates/pavis-relay/src/handlers.rs                       | 100.00%  | 167 / 167 |                                                                                 |
| crates/pavis-relay/src/ingest.rs                         | 60.00%   | 3 / 5     | 19-20                                                                           |
| crates/pavis-relay/src/main.rs                           | 0.00%    | 0 / 5     | 14-18                                                                           |
| crates/pavis-relay/src/pipeline.rs                       | 90.59%   | 77 / 85   | 21, 69-75, 91-95, 106, 112, 118, 136, 184                                       |
| crates/pavis-relay/src/routes.rs                         | 94.74%   | 18 / 19   | 32                                                                              |
| crates/pavis-relay/src/state.rs                          | 95.24%   | 140 / 147 | 218-219, 223-228                                                                |
| crates/pavis-testkit/src/bin/pavis-mock-relay.rs         | 0.00%    | 0 / 4     | 6-9                                                                             |
| crates/pavis-testkit/src/bin/pavis-mock-upstream.rs      | 0.00%    | 0 / 4     | 6-9                                                                             |
| crates/pavis-testkit/src/common/logging.rs               | 0.00%    | 0 / 3     | 3-5                                                                             |
| crates/pavis-testkit/src/common/shutdown.rs              | 0.00%    | 0 / 10    | 3-23                                                                            |
| crates/pavis-testkit/src/relay/routes/longpoll.rs        | 0.00%    | 0 / 25    | 17-60                                                                           |
| crates/pavis-testkit/src/relay/routes/mod.rs             | 0.00%    | 0 / 6     | 11-16                                                                           |
| crates/pavis-testkit/src/relay/routes/publish.rs         | 0.00%    | 0 / 8     | 11-25                                                                           |
| crates/pavis-testkit/src/relay/routes/status.rs          | 0.00%    | 0 / 4     | 9-12                                                                            |
| crates/pavis-testkit/src/relay/server.rs                 | 0.00%    | 0 / 18    | 9-38                                                                            |
| crates/pavis-testkit/src/relay/state.rs                  | 0.00%    | 0 / 24    | 26-76                                                                           |
| crates/pavis-testkit/src/upstream/routes/delay.rs        | 0.00%    | 0 / 7     | 14-22                                                                           |
| crates/pavis-testkit/src/upstream/routes/echo.rs         | 23.08%   | 9 / 39    | 13-64                                                                           |
| crates/pavis-testkit/src/upstream/routes/healthz.rs      | 0.00%    | 0 / 2     | 5-6                                                                             |
| crates/pavis-testkit/src/upstream/routes/mod.rs          | 0.00%    | 0 / 71    | 32-210                                                                          |
| crates/pavis-testkit/src/upstream/routes/reset.rs        | 0.00%    | 0 / 6     | 5-11                                                                            |
| crates/pavis-testkit/src/upstream/routes/status.rs       | 0.00%    | 0 / 9     | 11-20                                                                           |
| crates/pavis-testkit/src/upstream/server.rs              | 0.00%    | 0 / 55    | 12-107                                                                          |
| crates/pavis-testkit/src/upstream/tls.rs                 | 0.00%    | 0 / 13    | 14-34                                                                           |
| crates/pavis/src/agent/backoff.rs                        | 100.00%  | 7 / 7     |                                                                                 |
| crates/pavis/src/agent/lkg.rs                            | 100.00%  | 30 / 30   |                                                                                 |
| crates/pavis/src/agent/worker/agent.rs                   | 92.42%   | 61 / 66   | 143, 165-167, 175                                                               |
| crates/pavis/src/load.rs                                 | 100.00%  | 8 / 8     |                                                                                 |
| crates/pavis/src/main.rs                                 | 62.77%   | 59 / 94   | 35, 69-72, 77, 84, 87, 93-96, 115-124, 144-149, 157-184, 191, 199-200           |
| crates/pavis/src/proxy/header_ops.rs                     | 86.79%   | 92 / 106  | 89, 128-140, 164, 175, 191                                                      |
| crates/pavis/src/proxy/identity.rs                       | 92.86%   | 13 / 14   | 32                                                                              |
| crates/pavis/src/proxy/service.rs                        | 81.16%   | 56 / 69   | 54, 61-64, 82, 91-92, 96-98, 106, 190, 413                                      |
| crates/pavis/src/router.rs                               | 95.24%   | 40 / 42   | 89, 97                                                                          |
| crates/pavis/src/router/matcher.rs                       | 96.08%   | 49 / 51   | 27, 72                                                                          |
| crates/pavis/src/state.rs                                | 100.00%  | 15 / 15   |                                                                                 |
| crates/pavis/src/telemetry.rs                            | 100.00%  | 5 / 5     |                                                                                 |
| crates/pavis/src/telemetry/access_log.rs                 | 100.00%  | 40 / 40   |                                                                                 |
| crates/pavis/src/upstream.rs                             | 100.00%  | 8 / 8     |                                                                                 |
| crates/pavis/src/upstream/cluster.rs                     | 100.00%  | 27 / 27   |                                                                                 |
| crates/pavis/src/upstream/load_balance.rs                | 85.00%   | 17 / 20   | 30-32                                                                           |
| crates/pavis/src/upstream/resolver.rs                    | 81.42%   | 92 / 113  | 88-90, 129, 132, 139-140, 144-147, 166, 174, 186, 208-232, 240                  |

Total coverage: 81.58%