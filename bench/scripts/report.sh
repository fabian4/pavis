#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_ENV="${CONFIG_ENV:-$SCRIPT_DIR/../config/config.env}"
# shellcheck source=bench/config/config.env
if [[ -f "$CONFIG_ENV" ]]; then
  source "$CONFIG_ENV"
fi

if [[ -f "${SCRIPT_DIR}/summarize_github.sh" ]]; then
  bash "${SCRIPT_DIR}/summarize_github.sh"
fi

STANDALONE_CSV="${STANDALONE_CSV:-bench/output/standalone/summary.csv}"
SYSTEM_CSV="${SYSTEM_CSV:-bench/output/system/summary.csv}"

profile="${BENCH_PROFILE:-}"
profile_from_env=0
if [[ -n "$profile" ]]; then
  profile_from_env=1
fi
if [[ -z "$profile" ]]; then
  profile="workstation"
fi

if [[ "$profile" == "ci" ]]; then
  profile="github"
fi

has_profile_rows() {
  local csv="$1"
  local want_profile="$2"
  local want_mode="$3"
  if [[ ! -f "$csv" ]]; then
    return 1
  fi
  awk -F, -v p="$want_profile" -v m="$want_mode" '
    NR==1 {
      for (i=1; i<=NF; i++) {
        h=$i
        gsub(/^[ \t"]+|[ \t"]+$/, "", h)
        h=tolower(h)
        if (h=="bench_profile") cp=i
        if (h=="bench_mode") cm=i
      }
      next
    }
    {
      if (!cp || !cm) next
      prof=$(cp); mode=$(cm)
      gsub(/^[ \t"]+|[ \t"]+$/, "", prof)
      gsub(/^[ \t"]+|[ \t"]+$/, "", mode)
      if (prof==p && mode==m) { found=1; exit }
    }
    END { exit found ? 0 : 1 }
  ' "$csv"
}

if [[ "$profile" == "workstation" && "$profile_from_env" -eq 0 ]]; then
  if ! has_profile_rows "$STANDALONE_CSV" "workstation" "standalone"; then
    if has_profile_rows "$STANDALONE_CSV" "github" "standalone"; then
      profile="github"
    fi
  fi
fi

if [[ "$profile" == "github" ]]; then
  REPORT_OUT="${REPORT_OUT:-bench/output/report.github.md}"

  CONTEXT_ENV_STANDALONE="${CONTEXT_ENV_STANDALONE:-bench/output/standalone/context.env}"
  CONTEXT_ENV_SYSTEM="${CONTEXT_ENV_SYSTEM:-bench/output/system/context.env}"

  if [[ -f "$CONTEXT_ENV_STANDALONE" ]]; then
    # shellcheck source=/dev/null
    source "$CONTEXT_ENV_STANDALONE"
  elif [[ -f "$CONTEXT_ENV_SYSTEM" ]]; then
    # shellcheck source=/dev/null
    source "$CONTEXT_ENV_SYSTEM"
  fi

  profile="${BENCH_PROFILE:-}"
  bench_proxy="${BENCH_PROXY:-}"
  payload="${BENCH_PAYLOAD_SIZE:-}"
  tls="${BENCH_TLS:-}"
  metrics="${BENCH_METRICS:-}"
  git_sha="${GIT_SHA:-}"
  host_cores="${BENCH_HOST_CORES:-}"
  host_cpuset="${BENCH_HOST_CPUSET_EFFECTIVE:-}"
  host_mem_kib="${BENCH_HOST_MEM_TOTAL:-}"
  kernel="${BENCH_HOST_KERNEL:-}"
  cpu_model="${BENCH_HOST_CPU_MODEL:-}"

  awk -v standalone_csv="$STANDALONE_CSV" \
    -v system_csv="$SYSTEM_CSV" \
    -v profile="$profile" \
    -v bench_proxy="$bench_proxy" \
    -v payload="$payload" \
    -v tls="$tls" \
    -v metrics="$metrics" \
    -v git_sha="$git_sha" \
    -v host_cores="$host_cores" \
    -v host_cpuset="$host_cpuset" \
    -v host_mem_kib="$host_mem_kib" \
    -v kernel="$kernel" \
    -v cpu_model="$cpu_model" \
    -v rollback_ttbr_threshold_ms="${ROLLBACK_TTBR_THRESHOLD_MS:-1000}" '
  function csv_split(line, out,    i,c,inq,field,n,len) {
    n=0; field=""; inq=0; len=length(line)
    for (i=1; i<=len; i++) {
      c=substr(line,i,1)
      if (inq) {
        if (c=="\"") {
          if (i < len && substr(line,i+1,1)=="\"") {
            field=field "\""
            i++
          } else {
            inq=0
          }
        } else {
          field=field c
        }
      } else {
        if (c=="\"") {
          inq=1
        } else if (c==",") {
          out[++n]=field
          field=""
        } else {
          field=field c
        }
      }
    }
    out[++n]=field
    return n
  }
  function trim(s) { sub(/^[ \t]+/,"",s); sub(/[ \t]+$/,"",s); return s }
  function get(f, name) { return f[col[name]] }
  function dequote(x) { gsub(/^"|"$/, "", x); return x }
  function is_num(x) { return x ~ /^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$/ }
  function render(x) {
    x = dequote(x)
    if (x == "" || x == "null") return "—"
    return x
  }
  function mem_gib(x,    v) {
    if (!is_num(x)) return x
    v = int((x + 0) / 1024 / 1024 + 0.5)
    return v "GiB"
  }
  function cpu_trim(x) {
    sub(/ Processor$/, "", x)
    sub(/[[:space:]]+$/, "", x)
    return x
  }
  function gate_row(domain, case_name, check, value, threshold, result) {
    if (is_gate_excluded(domain, case_name, check)) {
      return
    }
    gate_rows[++gate_count,"domain"]=domain
    gate_rows[gate_count,"case"]=case_name
    gate_rows[gate_count,"check"]=check
    gate_rows[gate_count,"value"]=value
    gate_rows[gate_count,"threshold"]=threshold
    gate_rows[gate_count,"result"]=result
  }
  function worst_result(curr, candidate) {
    if (curr == "FAIL" || candidate == "FAIL") return "FAIL"
    if (curr == "WARN" || candidate == "WARN") return "WARN"
    return "PASS"
  }
  function is_gate_excluded(domain, case_name, check) {
    if (domain == "system" && case_name == "rollback_performance" && check == "rollback_ttbr_ms") return 1
    if (domain == "system" && case_name == "stress_recovery" && check == "latency_regression_pct") return 1
    return 0
  }
  function result_label(res) {
    if (res == "FAIL") return "❌ FAIL"
    if (res == "WARN") return "⚠️ WARN"
    return "✅ PASS"
  }
  function read_csv(file, mode,    line, n, i) {
    if (file == "") return
    if ((getline line < file) <= 0) return
    n=csv_split(line, f)
    for (i=1; i<=n; i++) {
      col[trim(f[i])] = i
    }
    while ((getline line < file) > 0) {
      n=csv_split(line, f)
      if (mode == "standalone") {
        if (dequote(get(f,"bench_profile")) != "github") continue
        if (dequote(get(f,"bench_mode")) != "standalone") continue
        case_name=dequote(get(f,"case"))
        if (case_name == "") continue
        data_s[case_name,"case"]=case_name
        data_s[case_name,"target_rps"]=get(f,"target_rps")
        data_s[case_name,"achieved_rps"]=get(f,"achieved_rps")
        data_s[case_name,"p99_ms"]=get(f,"p99_ms")
        data_s[case_name,"errors"]=get(f,"errors")
        data_s[case_name,"dropped"]=get(f,"dropped")
        data_s[case_name,"rps_iqr"]=get(f,"rps_iqr")
        data_s[case_name,"proxy_cpu_avg"]=get(f,"proxy_cpu_avg")
        data_s[case_name,"proxy_mem_peak_mib"]=get(f,"proxy_mem_peak_mib")
      } else if (mode == "system") {
        if (dequote(get(f,"bench_mode")) != "system") continue
        case_name=dequote(get(f,"case"))
        if (case_name == "") continue
        data_sys[case_name,"case"]=case_name
        data_sys[case_name,"target_rps"]=get(f,"target_rps")
        data_sys[case_name,"baseline_rps"]=get(f,"baseline_rps")
        data_sys[case_name,"stress_rps"]=get(f,"stress_rps")
        data_sys[case_name,"baseline_p99_ms"]=get(f,"baseline_p99_ms")
        data_sys[case_name,"stress_p99_ms"]=get(f,"stress_p99_ms")
        data_sys[case_name,"recovery_p99_ms"]=get(f,"recovery_p99_ms")
        data_sys[case_name,"transition_p99_ms"]=get(f,"transition_p99_ms")
        data_sys[case_name,"p99_delta_ms"]=get(f,"p99_delta_ms")
        data_sys[case_name,"convergence_time_ms"]=get(f,"convergence_time_ms")
        data_sys[case_name,"rollback_ttbr_ms"]=get(f,"rollback_ttbr_ms")
        data_sys[case_name,"baseline_restored"]=get(f,"baseline_restored")
        data_sys[case_name,"latency_regression_pct"]=get(f,"latency_regression_pct")
        data_sys[case_name,"rss_growth_mb"]=get(f,"rss_growth_mb")
        data_sys[case_name,"errors"]=get(f,"errors")
        data_sys[case_name,"config_versions"]=get(f,"config_versions")
      }
    }
    close(file)
  }
  BEGIN {
    read_csv(standalone_csv, "standalone")
    delete col
    read_csv(system_csv, "system")

    overall="PASS"
    if (("latency_short_1x" SUBSEP "case") in data_s) {
      val = render(data_s["latency_short_1x","dropped"])
      if (is_num(val)) {
        res = (val + 0 > 100) ? "WARN" : "PASS"
        overall = worst_result(overall, res)
        gate_row("standalone", "latency_short_1x", "dropped", val, "≤ 100", result_label(res))
      }
    }
    if (("concurrency_short_1x" SUBSEP "case") in data_s) {
      val = render(data_s["concurrency_short_1x","errors"])
      if (is_num(val)) {
        res = (val + 0 > 0) ? "WARN" : "PASS"
        overall = worst_result(overall, res)
        gate_row("standalone", "concurrency_short_1x", "errors", val, "= 0", result_label(res))
      }
    }
    if (("throughput_short_1x" SUBSEP "case") in data_s) {
      val = render(data_s["throughput_short_1x","errors"])
      if (is_num(val)) {
        res = (val + 0 > 0) ? "FAIL" : "PASS"
        overall = worst_result(overall, res)
        gate_row("standalone", "throughput_short_1x", "errors", val, "= 0", result_label(res))
      }
    }
    if (("config_reload_convergence" SUBSEP "case") in data_sys) {
      val = render(data_sys["config_reload_convergence","convergence_time_ms"])
      if (is_num(val)) {
        res = (val + 0 > 250) ? "FAIL" : "PASS"
        overall = worst_result(overall, res)
        gate_row("system", "config_reload_convergence", "convergence_time_ms", val, "≤ 250", result_label(res))
      }
    }
    if (("rollback_performance" SUBSEP "case") in data_sys) {
      val = render(data_sys["rollback_performance","rollback_ttbr_ms"])
      if (is_num(val)) {
        res = (val + 0 > rollback_ttbr_threshold_ms + 0) ? "FAIL" : "PASS"
      } else {
        res = "FAIL"
      }
      if (!is_gate_excluded("system", "rollback_performance", "rollback_ttbr_ms")) {
        overall = worst_result(overall, res)
        gate_row("system", "rollback_performance", "rollback_ttbr_ms", val, "≤ " rollback_ttbr_threshold_ms, result_label(res))
      }
    }
    if (("stress_recovery" SUBSEP "case") in data_sys) {
      val = render(data_sys["stress_recovery","latency_regression_pct"])
      if (is_num(val)) {
        res = (val + 0 > 20) ? "FAIL" : "PASS"
        if (!is_gate_excluded("system", "stress_recovery", "latency_regression_pct")) {
          overall = worst_result(overall, res)
          gate_row("system", "stress_recovery", "latency_regression_pct", val, "≤ 20", result_label(res))
        }
      }
    }

    print "# 🧪 CI Benchmark Summary"
    print ""
    print "CI-grade benchmark output.  "
    print "Intended for **health checks and regression detection only**.  "
    print ""
    print "---"
    print ""
    print "## Overall Gate Summary"
    print ""
    if (gate_count > 0) {
      print "| domain | case | check | value | threshold | result |"
      print "|------|------|-------|-------|-----------|--------|"
      for (i=1; i<=gate_count; i++) {
        print "| " gate_rows[i,"domain"] " | " gate_rows[i,"case"] " | " gate_rows[i,"check"] \
          " | " gate_rows[i,"value"] " | " gate_rows[i,"threshold"] " | " gate_rows[i,"result"] " |"
      }
    }
    print ""
    print "Notes:"
    print "- **overall status** is implied as the worst `result` in this table."
    print "- Tables below are **observations only** (no per-case gating)."
    print ""
    print "---"
    print ""
    print "## Standalone · Data Plane (Observations)"
    print ""
    print "### Throughput / Capacity"
    print ""
    print "| case | achieved_rps | rps_iqr | errors | dropped | proxy_cpu_avg | proxy_mem_peak_mib |"
    print "|------|--------------|---------|--------|---------|---------------|--------------------|"
    order_s[1]="throughput_short_1x"
    order_s[2]="concurrency_short_1x"
    order_s[3]="churn_short_1x"
    for (i=1; i<=3; i++) {
      case_name=order_s[i]
      if (!(case_name SUBSEP "case" in data_s)) continue
      printf "| %s | %s | %s | %s | %s | %s | %s |\n",
        case_name,
        render(data_s[case_name,"achieved_rps"]),
        render(data_s[case_name,"rps_iqr"]),
        render(data_s[case_name,"errors"]),
        render(data_s[case_name,"dropped"]),
        render(data_s[case_name,"proxy_cpu_avg"]),
        render(data_s[case_name,"proxy_mem_peak_mib"])
    }
    print ""
    print "### Latency / Saturation"
    print ""
    if (("latency_short_1x" SUBSEP "case") in data_s) {
      print "| case | target_rps | achieved_rps | p99_ms | dropped | errors | proxy_mem_peak_mib |"
      print "|------|------------|--------------|--------|---------|--------|--------------------|"
      case_name="latency_short_1x"
      printf "| %s | %s | %s | %s | %s | %s | %s |\n",
        case_name,
        render(data_s[case_name,"target_rps"]),
        render(data_s[case_name,"achieved_rps"]),
        render(data_s[case_name,"p99_ms"]),
        render(data_s[case_name,"dropped"]),
        render(data_s[case_name,"errors"]),
        render(data_s[case_name,"proxy_mem_peak_mib"])
    }
    print ""
    print "---"
    print ""
    print "## System · Lifecycle (Observations)"
    print ""

    if (("config_reload_convergence" SUBSEP "case") in data_sys) {
      print "### Config Reload · Convergence"
      print ""
      print "| target_rps | baseline_p99_ms | transition_p99_ms | p99_delta_ms | convergence_time_ms | errors |"
      print "|------------|-----------------|-------------------|--------------|---------------------|--------|"
      print "| " render(data_sys["config_reload_convergence","target_rps"]) " | " \
            render(data_sys["config_reload_convergence","baseline_p99_ms"]) " | " \
            render(data_sys["config_reload_convergence","transition_p99_ms"]) " | " \
            render(data_sys["config_reload_convergence","p99_delta_ms"]) " | " \
            render(data_sys["config_reload_convergence","convergence_time_ms"]) " | " \
            (render(data_sys["config_reload_convergence","errors"])=="—" ? "0" : render(data_sys["config_reload_convergence","errors"])) " |"
      print ""
      print "---"
      print ""
    }

    if (("rollback_performance" SUBSEP "case") in data_sys) {
      print "### Rollback · Performance"
      print ""
      print "| target_rps | baseline_p99_ms | rollback_p99_ms | rollback_ttbr_ms | config_versions | errors |"
      print "|------------|-----------------|-----------------|------------------|-----------------|--------|"
      print "| " render(data_sys["rollback_performance","target_rps"]) " | " \
            render(data_sys["rollback_performance","baseline_p99_ms"]) " | " \
            render(data_sys["rollback_performance","recovery_p99_ms"]) " | " \
            render(data_sys["rollback_performance","rollback_ttbr_ms"]) " | " \
            render(data_sys["rollback_performance","config_versions"]) " | " \
            (render(data_sys["rollback_performance","errors"])=="—" ? "0" : render(data_sys["rollback_performance","errors"])) " |"
      print ""
      print "---"
      print ""
    }

    if (("stress_recovery" SUBSEP "case") in data_sys) {
      print "### Stress → Recovery"
      print ""
      print "| baseline_rps | stress_rps | stress_p99_ms | recovery_p99_ms | latency_regression_pct | rss_growth_mb | errors |"
      print "|--------------|------------|---------------|------------------|----------------------|---------------|--------|"
      print "| " render(data_sys["stress_recovery","baseline_rps"]) " | " \
            render(data_sys["stress_recovery","stress_rps"]) " | " \
            render(data_sys["stress_recovery","stress_p99_ms"]) " | " \
            render(data_sys["stress_recovery","recovery_p99_ms"]) " | " \
            render(data_sys["stress_recovery","latency_regression_pct"]) " | " \
            render(data_sys["stress_recovery","rss_growth_mb"]) " | " \
            (render(data_sys["stress_recovery","errors"])=="—" ? "0" : render(data_sys["stress_recovery","errors"])) " |"
      print ""
      print "---"
      print ""
    }

    print "## Contract (CI Semantics)"
    print ""
    print "- **Standalone**: dataplane health & regression signals; CI noise expected."
    print "- **System**: lifecycle safety & control-plane invariants."
    print "- Only the **Overall Gate Summary** carries judgement."
    print ""
  }
  ' > "$REPORT_OUT"

  echo "Report written to $REPORT_OUT"
  exit 0
fi

case "$profile" in
  workstation)
    bash "${SCRIPT_DIR}/report_standalone_workstation.sh"
    ;;
  *)
    echo "error: unsupported BENCH_PROFILE=$profile (expected github or workstation)" >&2
    exit 1
    ;;
 esac
