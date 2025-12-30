| file                                                     | coverage | covered   | missed_lines              |
|----------------------------------------------------------|----------|-----------|---------------------------|
| crates/pavctl/src/commands.rs                            | 100.00%  | 4 / 4     |                           |
| crates/pavctl/src/commands/check.rs                      | 88.89%   | 8 / 9     | 13                        |
| crates/pavctl/src/commands/convert.rs                    | 78.95%   | 15 / 19   | 21-22, 35-36              |
| crates/pavctl/src/commands/gen.rs                        | 92.86%   | 13 / 14   | 15                        |
| crates/pavctl/src/commands/view.rs                       | 69.23%   | 9 / 13    | 20-23                     |
| crates/pavctl/src/format.rs                              | 97.01%   | 65 / 67   | 33, 61                    |
| crates/pavctl/src/main.rs                                | 70.83%   | 17 / 24   | 71-72, 89-93              |
| crates/pavctl/src/parse.rs                               | 100.00%  | 8 / 8     |                           |
| crates/pavctl/tests/pipeline.rs                          | 86.05%   | 37 / 43   | 141, 149, 156-160         |
| crates/pavis-codec-api/src/lib.rs                        | 100.00%  | 4 / 4     |                           |
| crates/pavis-codec-serde/src/config/convert.rs           | 100.00%  | 12 / 12   |                           |
| crates/pavis-codec-serde/src/config/convert/routes.rs    | 96.30%   | 78 / 81   | 45, 90-91                 |
| crates/pavis-codec-serde/src/config/convert/server.rs    | 75.00%   | 9 / 12    | 15-17                     |
| crates/pavis-codec-serde/src/config/convert/telemetry.rs | 65.52%   | 19 / 29   | 39-44, 50-51, 53-54       |
| crates/pavis-codec-serde/src/config/convert/upstreams.rs | 100.00%  | 52 / 52   |                           |
| crates/pavis-codec-serde/src/config/types.rs             | 80.00%   | 8 / 10    | 35-36                     |
| crates/pavis-codec-serde/src/config/types/upstreams.rs   | 100.00%  | 9 / 9     |                           |
| crates/pavis-codec-serde/src/config/validation.rs        | 100.00%  | 20 / 20   |                           |
| crates/pavis-codec-serde/src/lib.rs                      | 96.67%   | 29 / 30   | 50                        |
| crates/pavis-codec-serde/src/serde_helpers.rs            | 100.00%  | 10 / 10   |                           |
| crates/pavis-core/src/runtime.rs                         | 75.00%   | 6 / 8     | 51-52                     |
| crates/pavis-core/src/serde_impl.rs                      | 66.67%   | 14 / 21   | 12, 14, 26-30, 42, 54     |
| crates/pavis-core/src/validate.rs                        | 100.00%  | 5 / 5     |                           |
| crates/pavis-core/src/validate/headers.rs                | 100.00%  | 19 / 19   |                           |
| crates/pavis-core/src/validate/routes.rs                 | 100.00%  | 44 / 44   |                           |
| crates/pavis-core/src/validate/server.rs                 | 100.00%  | 6 / 6     |                           |
| crates/pavis-core/src/validate/upstreams.rs              | 100.00%  | 11 / 11   |                           |
| crates/pavis-e2e/src/support/pavis/http.rs               | 0.00%    | 0 / 7     | 85-94                     |
| crates/pavis-ingest-api/src/lib.rs                       | 100.00%  | 13 / 13   |                           |
| crates/pavis-pvs/src/header.rs                           | 94.12%   | 16 / 17   | 37                        |
| crates/pavis-pvs/src/read.rs                             | 95.24%   | 20 / 21   | 22                        |
| crates/pavis-pvs/src/verify.rs                           | 79.10%   | 53 / 67   | 30-31, 54-71, 78-79       |
| crates/pavis-pvs/src/write.rs                            | 100.00%  | 18 / 18   |                           |
| crates/pavis-relay/src/config/env.rs                     | 82.14%   | 23 / 28   | 8-10, 30-31               |
| crates/pavis-relay/src/config/load.rs                    | 70.37%   | 19 / 27   | 7-13, 38                  |
| crates/pavis-relay/src/config/tests.rs                   | 100.00%  | 4 / 4     |                           |
| crates/pavis-relay/src/handlers.rs                       | 92.68%   | 114 / 123 | 50, 88, 105, 144-150, 169 |
| crates/pavis-relay/src/main.rs                           | 0.00%    | 0 / 40    | 18-82                     |
| crates/pavis-relay/src/routes.rs                         | 57.89%   | 11 / 19   | 23-32                     |
| crates/pavis-relay/src/state.rs                          | 100.00%  | 50 / 50   |                           |
| crates/pavis-relay/tests/relay_http.rs                   | 100.00%  | 19 / 19   |                           |
| crates/pavis/src/load.rs                                 | 100.00%  | 7 / 7     |                           |
| crates/pavis/src/main.rs                                 | 83.05%   | 49 / 59   | 89-90, 94-97, 128-137     |
| crates/pavis/src/proxy/header_ops.rs                     | 71.43%   | 20 / 28   | 20-24, 47-51              |
| crates/pavis/src/proxy/service.rs                        | 58.33%   | 7 / 12    | 78-79, 155-164            |
| crates/pavis/src/proxy/service/service_tests.rs          | 100.00%  | 19 / 19   |                           |
| crates/pavis/src/router.rs                               | 100.00%  | 17 / 17   |                           |
| crates/pavis/src/router/matcher.rs                       | 100.00%  | 37 / 37   |                           |
| crates/pavis/src/telemetry.rs                            | 100.00%  | 5 / 5     |                           |
| crates/pavis/src/telemetry/access_log.rs                 | 25.81%   | 8 / 31    | 100-152                   |
| crates/pavis/src/upstream.rs                             | 100.00%  | 6 / 6     |                           |
| crates/pavis/src/upstream/cluster.rs                     | 100.00%  | 11 / 11   |                           |
| crates/pavis/src/upstream/load_balance.rs                | 90.00%   | 18 / 20   | 27, 40                    |
| crates/pavis/tests/cli_features.rs                       | 79.59%   | 39 / 49   | 13, 24, 32-38, 82, 88-89  |
| crates/pavis/tests/common.rs                             | 100.00%  | 5 / 5     |                           |
| crates/pavis/tests/config_integrity.rs                   | 78.57%   | 33 / 42   | 18, 26-30, 45, 64, 70-71  |

Total coverage: 84.62%
