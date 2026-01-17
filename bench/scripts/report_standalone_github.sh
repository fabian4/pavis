#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -z "${BENCH_SCRIPTS_DIR:-}" ]]; then
  BENCH_SCRIPTS_DIR="$SCRIPT_DIR"
fi
if [[ -f "${BENCH_SCRIPTS_DIR}/utils.sh" ]]; then
  # shellcheck source=bench/scripts/utils.sh
  source "${BENCH_SCRIPTS_DIR}/utils.sh"
fi

usage() {
  cat <<'USAGE'
Usage:
  bench/scripts/report_standalone_github.sh [--input <csv>] [--output <md>]

Environment:
  INPUT   Path to input CSV (default: bench/output/standalone/summary.csv)
  OUTPUT  Path to output markdown (default: bench/output/standalone/report.github.md)
  RUN_ID  Filter to a specific git sha (default: latest by timestamp)
USAGE
}

die() {
  echo "error: $*" >&2
  exit 1
}

input="${INPUT:-bench/output/standalone/summary.csv}"
output="${OUTPUT:-bench/output/standalone/report.github.md}"
run_id_env="${RUN_ID:-}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --input)
      input="$2"
      shift 2
      ;;
    --output)
      output="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown arg: $1"
      ;;
  esac
done

[ -f "$input" ] || die "input not found: $input"

out_dir=$(dirname "$output")
mkdir -p "$out_dir"

gen_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

awk -F, -v run_id_env="$run_id_env" -v gen_at="$gen_at" '
  function norm(h) {
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", h)
    return tolower(h)
  }
  function dequote(x) {
    gsub(/^"|"$/, "", x)
    return x
  }
  function is_num(x) {
    return x ~ /^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$/
  }
  function render(x) {
    x = dequote(x)
    if (x == "" || x == "null") return "—"
    return x
  }
  function fmt2(x) {
    if (!is_num(x)) return render(x)
    return sprintf("%.2f", x+0)
  }
  function fmt3(x) {
    if (!is_num(x)) return render(x)
    return sprintf("%.3f", x+0)
  }
  function with_suffix(val, suffix) {
    if (val == "—" || suffix == "") return val
    return val " " suffix
  }
  function loop_type(t) {
    if (t ~ /^wrk/ || t ~ /wrk-multi/) return "closed"
    if (t ~ /^loadgen/ || t ~ /loadgen/) return "open"
    return "—"
  }
  function verdict(loop, achieved, target, p99, errors, dropped) {
    if (achieved == "" || achieved == "null") return "FAIL"
    if (!is_num(achieved)) return "FAIL"
    if (errors != "" && is_num(errors) && errors + 0 > 0) return "WARN"
    if (loop == "open") {
      if (target != "" && is_num(target)) {
        if (achieved + 0 < (target + 0) * 0.75) return "FAIL"
        if (achieved + 0 < (target + 0) * 0.95) return "WARN"
      }
      if (p99 == "" || p99 == "null") return "FAIL"
      return "PASS"
    }
    if (dropped != "" && is_num(dropped) && dropped + 0 > 0) return "WARN"
    return "PASS"
  }
  function verdict_label(v) {
    if (v == "WARN") return "⚠️ WARN"
    if (v == "FAIL") return "❌ FAIL"
    return "PASS"
  }
  function host_mem_fmt(x) {
    if (x == "" || x == "null") return "—"
    if (x ~ /MiB$/) {
      sub(/MiB$/, " MiB", x)
    }
    return x
  }
  BEGIN {
    alias["git_sha"]="git_sha"
    alias["aggregate"]="aggregate"
    req_cols="git_sha timestamp proxy case type aggregate iteration runs achieved_rps p50_ms p90_ms p99_ms errors dropped rps_iqr p99_iqr target_rps proxy_cpu_avg proxy_mem_peak_mib bench_profile bench_mode bench_payload_size bench_tls bench_metrics cpu_model kernel"
    split(req_cols, req, " ")
  }
  NR==1 {
    for (i=1;i<=NF;i++) {
      h=norm($i)
      col[h]=i
    }
    for (i in req) {
      if (!col[req[i]]) {
        print "error: missing required column for report: " req[i] > "/dev/stderr"
        exit 2
      }
    }
    next
  }
  {
    profile=dequote($(col["bench_profile"]))
    if (profile != "github") next
    mode=dequote($(col["bench_mode"]))
    if (mode != "standalone") next
    row_count++

    rid=dequote($(col["git_sha"]))
    ts=dequote($(col["timestamp"]))
    if (rid=="" || ts=="") next
    if (run_id_env=="") {
      if (max_ts=="" || ts>max_ts) {
        max_ts=ts
        max_run=rid
      }
    }
  }
  END {
    if (row_count == 0) {
      print "error: no standalone/github rows found in input" > "/dev/stderr"
      exit 4
    }
    if (run_id_env=="" && max_run=="") {
      print "error: unable to determine run_id from input" > "/dev/stderr"
      exit 3
    }
  }
  NR>1 {
    profile=dequote($(col["bench_profile"]))
    if (profile != "github") next
    mode=dequote($(col["bench_mode"]))
    if (mode != "standalone") next
    rid=dequote($(col["git_sha"]))
    if (run_id_env!="" && rid != run_id_env) next
    if (run_id_env=="" && rid != max_run) next

    proxy=dequote($(col["proxy"]))
    proxies[proxy]=1

    if (context_set==0 && $(col["aggregate"])=="1") {
      context_set=1
      ctx["profile"]=profile
      ctx["mode"]=$(col["bench_mode"])
      ctx["payload"]=$(col["bench_payload_size"])
      ctx["tls"]=$(col["bench_tls"])
      ctx["metrics"]=$(col["bench_metrics"])
      ctx["git_sha"]=rid
      ctx["host_cores"]=$(col["bench_host_cores"])
      ctx["host_cpuset"]=$(col["bench_host_cpuset_effective"])
      ctx["host_mem"]=$(col["bench_host_mem_total"])
      ctx["proxy_cpu_limit"]=$(col["bench_proxy_cpu_limit"])
      ctx["proxy_mem_limit"]=$(col["bench_proxy_mem_limit"])
      ctx["kernel"]=$(col["kernel"])
      ctx["cpu_model"]=$(col["cpu_model"])
    }

    case_name=dequote($(col["case"]))
    case_type=dequote($(col["type"]))
    aggregate=$(col["aggregate"])
    iteration=$(col["iteration"])

    is_multi = case_type ~ /-multi$/
    if (is_multi) {
      if (aggregate != "1" || iteration != "0") next
    } else {
      if (aggregate != "1" || iteration != "1") next
    }

    data[case_name,"case"]=case_name
    data[case_name,"tool"]=case_type
    data[case_name,"loop"]=loop_type(case_type)
    data[case_name,"runs"]=$(col["runs"])
    data[case_name,"target_rps"]=$(col["target_rps"])
    data[case_name,"achieved_rps"]=$(col["achieved_rps"])
    data[case_name,"p50_ms"]=$(col["p50_ms"])
    data[case_name,"p90_ms"]=$(col["p90_ms"])
    data[case_name,"p99_ms"]=$(col["p99_ms"])
    data[case_name,"errors"]=$(col["errors"])
    data[case_name,"dropped"]=$(col["dropped"])
    data[case_name,"rps_iqr"]=$(col["rps_iqr"])
    data[case_name,"p99_iqr"]=$(col["p99_iqr"])
    data[case_name,"proxy_cpu_avg"]=$(col["proxy_cpu_avg"])
    data[case_name,"proxy_mem_peak_mib"]=$(col["proxy_mem_peak_mib"])
  }
  END {
    n=0
    for (p in proxies) n++
    if (n>1) {
      print "error: multiple proxies detected in github mode" > "/dev/stderr"
      exit 4
    }

    print "# 🧪 CI Benchmark Summary — Health & Regression Signal"
    print ""
    print "> CI-grade benchmark output.  "
    print "> Intended for **health checks and regression detection only**.  "
    print "> NOT suitable for cross-proxy performance comparison or publication."
    print ""
    print "---"
    print ""
    print "## Run Context"
    print ""
    print "- profile=`" render(ctx["profile"]) "` · mode=`" render(ctx["mode"]) "` · payload=`" render(ctx["payload"]) "` · tls=`" render(ctx["tls"]) "` · metrics=`" render(ctx["metrics"]) "`"
    print "- git_sha=`" render(ctx["git_sha"]) "`"
    print "- host: `" render(ctx["host_cores"]) " cores (cpuset=" render(ctx["host_cpuset"]) ")`, mem=`" host_mem_fmt(render(ctx["host_mem"])) "`"
    print "- proxy limits: cpu=`" render(ctx["proxy_cpu_limit"]) "`, mem=`" render(ctx["proxy_mem_limit"]) "`"
    print "- kernel=`" render(ctx["kernel"]) "` · cpu=`" render(ctx["cpu_model"]) "`"
    print ""
    print "---"
    print ""
    print "## Case Results"
    print ""
    print "| case                 | tool 🔧        | loop 🔁 | runs | target_rps 🎯 | achieved_rps 🚀 | p50_ms | p90_ms | p99_ms ⏱ | errors ❌ | dropped 📉 | rps_iqr | proxy_cpu_avg | proxy_mem_peak_mib |"
    print "|----------------------|----------------|---------|------|---------------|-----------------|--------|--------|----------|----------|------------|---------|---------------|--------------------|"

    order[1]="throughput_short_1x"
    order[2]="latency_short_1x"
    order[3]="latency_extended_1x"
    order[4]="concurrency_short_1x"
    order[5]="churn_short_1x"

    overall="PASS"
    best_achieved=""; best_p50=""; best_p90=""; best_p99=""; best_errors=""; best_dropped=""; best_rps_iqr=""; best_p99_iqr=""; best_cpu=""; best_mem="";

    for (i=1; i<=5; i++) {
      c=order[i]
      if (!(c SUBSEP "case" in data)) {
        continue
      }
      v = verdict(data[c,"loop"], data[c,"achieved_rps"], data[c,"target_rps"], data[c,"p99_ms"], data[c,"errors"], data[c,"dropped"])
      if (v == "FAIL") overall="FAIL"
      else if (v == "WARN" && overall != "FAIL") overall="WARN"

      if (is_num(data[c,"achieved_rps"])) { if (best_achieved=="" || data[c,"achieved_rps"]+0 > best_achieved+0) best_achieved=data[c,"achieved_rps"] }
      if (is_num(data[c,"p50_ms"])) { if (best_p50=="" || data[c,"p50_ms"]+0 < best_p50+0) best_p50=data[c,"p50_ms"] }
      if (is_num(data[c,"p90_ms"])) { if (best_p90=="" || data[c,"p90_ms"]+0 < best_p90+0) best_p90=data[c,"p90_ms"] }
      if (is_num(data[c,"p99_ms"])) { if (best_p99=="" || data[c,"p99_ms"]+0 < best_p99+0) best_p99=data[c,"p99_ms"] }
      if (is_num(data[c,"errors"])) { if (best_errors=="" || data[c,"errors"]+0 < best_errors+0) best_errors=data[c,"errors"] }
      if (is_num(data[c,"dropped"])) { if (best_dropped=="" || data[c,"dropped"]+0 < best_dropped+0) best_dropped=data[c,"dropped"] }
      if (is_num(data[c,"rps_iqr"])) { if (best_rps_iqr=="" || data[c,"rps_iqr"]+0 < best_rps_iqr+0) best_rps_iqr=data[c,"rps_iqr"] }
      if (is_num(data[c,"p99_iqr"])) { if (best_p99_iqr=="" || data[c,"p99_iqr"]+0 < best_p99_iqr+0) best_p99_iqr=data[c,"p99_iqr"] }
      if (is_num(data[c,"proxy_cpu_avg"])) { if (best_cpu=="" || data[c,"proxy_cpu_avg"]+0 < best_cpu+0) best_cpu=data[c,"proxy_cpu_avg"] }
      if (is_num(data[c,"proxy_mem_peak_mib"])) { if (best_mem=="" || data[c,"proxy_mem_peak_mib"]+0 < best_mem+0) best_mem=data[c,"proxy_mem_peak_mib"] }

      printf "| %-20s | %-14s | %-7s | %-4s | %-13s | %-15s | %-6s | %-6s | %-8s | %-8s | %-10s | %-7s | %-13s | %-18s |\n",
        data[c,"case"],
        render(data[c,"tool"]),
        render(data[c,"loop"]),
        render(data[c,"runs"]),
        render(data[c,"target_rps"]),
        (is_num(data[c,"achieved_rps"]) && best_achieved!="" && data[c,"achieved_rps"]+0 < best_achieved+0) ? with_suffix(fmt2(data[c,"achieved_rps"]), "↓") : fmt2(data[c,"achieved_rps"]),
        (is_num(data[c,"p50_ms"]) && best_p50!="" && data[c,"p50_ms"]+0 > best_p50+0) ? with_suffix(fmt3(data[c,"p50_ms"]), "↑") : fmt3(data[c,"p50_ms"]),
        (is_num(data[c,"p90_ms"]) && best_p90!="" && data[c,"p90_ms"]+0 > best_p90+0) ? with_suffix(fmt3(data[c,"p90_ms"]), "↑") : fmt3(data[c,"p90_ms"]),
        (is_num(data[c,"p99_ms"]) && best_p99!="" && data[c,"p99_ms"]+0 > best_p99+0) ? with_suffix(fmt3(data[c,"p99_ms"]), "↑") : fmt3(data[c,"p99_ms"]),
        (is_num(data[c,"errors"]) && best_errors!="" && data[c,"errors"]+0 > best_errors+0) ? with_suffix(render(data[c,"errors"]), "↑") : render(data[c,"errors"]),
        (is_num(data[c,"dropped"]) && best_dropped!="" && data[c,"dropped"]+0 > best_dropped+0) ? with_suffix(render(data[c,"dropped"]), "↑") : render(data[c,"dropped"]),
        (is_num(data[c,"rps_iqr"]) && best_rps_iqr!="" && data[c,"rps_iqr"]+0 > best_rps_iqr+0) ? with_suffix(fmt2(data[c,"rps_iqr"]), "↑") : fmt2(data[c,"rps_iqr"]),
        (is_num(data[c,"proxy_cpu_avg"]) && best_cpu!="" && data[c,"proxy_cpu_avg"]+0 > best_cpu+0) ? with_suffix(fmt2(data[c,"proxy_cpu_avg"]), "↑") : fmt2(data[c,"proxy_cpu_avg"]),
        (is_num(data[c,"proxy_mem_peak_mib"]) && best_mem!="" && data[c,"proxy_mem_peak_mib"]+0 >= best_mem+0*1.5) ? with_suffix(fmt2(data[c,"proxy_mem_peak_mib"]), "⚠︎") : (is_num(data[c,"proxy_mem_peak_mib"]) && best_mem!="" && data[c,"proxy_mem_peak_mib"]+0 > best_mem+0) ? with_suffix(fmt2(data[c,"proxy_mem_peak_mib"]), "↑") : fmt2(data[c,"proxy_mem_peak_mib"])
    }

    print ""
    print "---"
    print ""
    print "## Interpretation Notes"
    print ""
    print "- **GitHub / CI mode**"
    print "    - Runs on shared, resource-constrained runners."
    print "    - CPU and memory limits are intentionally **unset**."
    print "    - Results provide **trend and health signals**, not capacity limits."
    print ""
    print "- **Verdict semantics**"
    print "    - `PASS`: within expected CI bounds."
    print "    - `WARN`: acceptable but worth tracking for regression (e.g. connection pressure)."
    print "    - `FAIL`: hard error or instability."
    print ""
    print "- **Workload semantics**"
    print "    - `closed-loop` (wrk): `dropped` not applicable."
    print "    - `open-loop` (loadgen): `dropped` indicates saturation/backpressure."
    print "    - Multi-run cases report median values from `aggregate=1` rows."
    print ""
    print "- **Multi-proxy benchmarks**"
    print "    - CI output structure is intentionally proxy-agnostic."
    print "    - When multiple proxies are enabled, each proxy produces an identical table."
    print "    - Cross-proxy comparison is **explicitly deferred** to workstation / publish profiles."
    print ""
  }
' "$input" > "$output"

echo "wrote: $output"
