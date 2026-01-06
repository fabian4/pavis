| file                                                     | coverage | covered   | missed_lines                                                                                 |
|----------------------------------------------------------|----------|-----------|----------------------------------------------------------------------------------------------|
| crates/pavctl/src/commands.rs                            | 100.00%  | 4 / 4     |                                                                                              |
| crates/pavctl/src/commands/check.rs                      | 100.00%  | 9 / 9     |                                                                                              |
| crates/pavctl/src/commands/convert.rs                    | 100.00%  | 18 / 18   |                                                                                              |
| crates/pavctl/src/commands/gen.rs                        | 100.00%  | 24 / 24   |                                                                                              |
| crates/pavctl/src/commands/view.rs                       | 100.00%  | 13 / 13   |                                                                                              |
| crates/pavctl/src/format.rs                              | 91.46%   | 75 / 82   | 30, 51, 78-82, 103                                                                           |
| crates/pavctl/src/main.rs                                | 100.00%  | 26 / 26   |                                                                                              |
| crates/pavctl/src/parse.rs                               | 100.00%  | 9 / 9     |                                                                                              |
| crates/pavis-codec-api/src/lib.rs                        | 53.33%   | 8 / 15    | 41-189, 216                                                                                  |
| crates/pavis-codec-serde/src/config/convert.rs           | 100.00%  | 25 / 25   |                                                                                              |
| crates/pavis-codec-serde/src/config/convert/routes.rs    | 73.33%   | 132 / 180 | 218, 230-233, 261, 279-294, 305-308, 354-357, 380, 398, 402, 410, 433-443, 499-515, 527, 533 |
| crates/pavis-codec-serde/src/config/convert/server.rs    | 100.00%  | 37 / 37   |                                                                                              |
| crates/pavis-codec-serde/src/config/convert/telemetry.rs | 96.00%   | 48 / 50   | 213, 217                                                                                     |
| crates/pavis-codec-serde/src/config/convert/upstreams.rs | 90.71%   | 127 / 140 | 173-174, 196-199, 210, 215, 285, 287, 290, 340, 349                                          |
| crates/pavis-codec-serde/src/config/types.rs             | 100.00%  | 11 / 11   |                                                                                              |
| crates/pavis-codec-serde/src/config/validation.rs        | 90.32%   | 28 / 31   | 69, 71-72                                                                                    |
| crates/pavis-codec-serde/src/lib.rs                      | 70.00%   | 14 / 20   | 40-41, 45-47, 60                                                                             |
| crates/pavis-codec-serde/src/serde_helpers.rs            | 100.00%  | 10 / 10   |                                                                                              |
| crates/pavis-core/src/runtime.rs                         | 88.89%   | 8 / 9     | 64                                                                                           |
| crates/pavis-core/src/serde_impl.rs                      | 95.24%   | 20 / 21   | 26                                                                                           |
| crates/pavis-core/src/validate.rs                        | 100.00%  | 6 / 6     |                                                                                              |
| crates/pavis-core/src/validate/headers.rs                | 93.33%   | 28 / 30   | 38, 41                                                                                       |
| crates/pavis-core/src/validate/routes.rs                 | 94.64%   | 53 / 56   | 100, 108-109                                                                                 |
| crates/pavis-core/src/validate/server.rs                 | 100.00%  | 7 / 7     |                                                                                              |
| crates/pavis-core/src/validate/upstreams.rs              | 100.00%  | 9 / 9     |                                                                                              |
| crates/pavis-ingest-api/src/lib.rs                       | 100.00%  | 13 / 13   |                                                                                              |
| crates/pavis-ingest-file/src/lib.rs                      | 92.86%   | 39 / 42   | 55-57                                                                                        |
| crates/pavis-ingest-file/src/watch.rs                    | 78.67%   | 59 / 75   | 45, 69-71, 85, 96-97, 105-110, 116-117, 123-124                                              |
| crates/pavis-pvs/src/header.rs                           | 100.00%  | 17 / 17   |                                                                                              |
| crates/pavis-pvs/src/read.rs                             | 100.00%  | 21 / 21   |                                                                                              |
| crates/pavis-pvs/src/verify.rs                           | 80.21%   | 77 / 96   | 57-69, 102, 109, 129-135, 186-188                                                            |
| crates/pavis-pvs/src/write.rs                            | 100.00%  | 21 / 21   |                                                                                              |
| crates/pavis-relay/src/app.rs                            | 100.00%  | 62 / 62   |                                                                                              |
| crates/pavis-relay/src/config/env.rs                     | 100.00%  | 27 / 27   |                                                                                              |
| crates/pavis-relay/src/config/load.rs                    | 100.00%  | 31 / 31   |                                                                                              |
| crates/pavis-relay/src/config/types.rs                   | 100.00%  | 57 / 57   |                                                                                              |
| crates/pavis-relay/src/handlers.rs                       | 97.21%   | 174 / 179 | 163, 173-176                                                                                 |
| crates/pavis-relay/src/ingest.rs                         | 60.00%   | 3 / 5     | 19-20                                                                                        |
| crates/pavis-relay/src/main.rs                           | 0.00%    | 0 / 5     | 14-18                                                                                        |
| crates/pavis-relay/src/pipeline.rs                       | 83.53%   | 71 / 85   | 21, 69-75, 91-95, 105-106, 112-118, 136, 184                                                 |
| crates/pavis-relay/src/routes.rs                         | 94.74%   | 18 / 19   | 32                                                                                           |
| crates/pavis-relay/src/state.rs                          | 97.64%   | 207 / 212 | 249-250, 374-375, 451                                                                        |
| crates/pavis/src/agent/backoff.rs                        | 100.00%  | 7 / 7     |                                                                                              |
| crates/pavis/src/agent/lkg.rs                            | 100.00%  | 30 / 30   |                                                                                              |
| crates/pavis/src/agent/worker/agent.rs                   | 92.42%   | 61 / 66   | 142, 164-166, 174                                                                            |
| crates/pavis/src/load.rs                                 | 100.00%  | 8 / 8     |                                                                                              |
| crates/pavis/src/main.rs                                 | 72.15%   | 57 / 79   | 111-112, 117, 124, 131-134, 153-162, 182-186, 194, 202-203                                   |
| crates/pavis/src/proxy/header_ops.rs                     | 83.65%   | 87 / 104  | 48-49, 89, 128-140, 173, 186, 197-198                                                        |
| crates/pavis/src/proxy/service.rs                        | 80.33%   | 49 / 61   | 59-62, 76-82, 89-90, 94-96                                                                   |
| crates/pavis/src/router.rs                               | 96.00%   | 48 / 50   | 93, 121                                                                                      |
| crates/pavis/src/router/matcher.rs                       | 82.46%   | 47 / 57   | 64-75, 81                                                                                    |
| crates/pavis/src/state.rs                                | 100.00%  | 15 / 15   |                                                                                              |
| crates/pavis/src/telemetry.rs                            | 100.00%  | 5 / 5     |                                                                                              |
| crates/pavis/src/telemetry/access_log.rs                 | 100.00%  | 40 / 40   |                                                                                              |
| crates/pavis/src/upstream.rs                             | 100.00%  | 8 / 8     |                                                                                              |
| crates/pavis/src/upstream/cluster.rs                     | 100.00%  | 27 / 27   |                                                                                              |
| crates/pavis/src/upstream/load_balance.rs                | 94.44%   | 17 / 18   | 38                                                                                           |
| crates/pavis/src/upstream/resolver.rs                    | 35.14%   | 39 / 111  | 30-42, 64-75, 81-90, 128-129, 133-145, 164, 169-196, 206-225, 236, 241                       |

Total coverage: 87.82%
