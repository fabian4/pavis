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
  bench/scripts/report.sh [--input <csv>] [--output <md>]

Environment:
  INPUT   Path to input CSV (default: bench/output/summary.csv)
  OUTPUT  Path to output markdown (default: bench/output/report.md)
  RUN_ID  Filter to a specific run_id; otherwise latest by timestamp
USAGE
}

die() {
  echo "error: $*" >&2
  exit 1
}

input="${INPUT:-bench/output/summary.csv}"
output="${OUTPUT:-bench/output/report.md}"
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
command -v jq >/dev/null 2>&1 || die "jq is required to parse result.json"

host_info_content=""
if [ -n "${BENCH_HOST_INFO:-}" ] && [ -f "$BENCH_HOST_INFO" ]; then
  host_info_content=$(cat "$BENCH_HOST_INFO")
fi
env_details="backend_cpuset=${BACKEND_CPUSET:-auto}, proxy_cpuset=${PROXY_CPUSET:-auto}, docker_compose=${BENCH_DOCKER_COMPOSE:-unknown}, output_dir=${BENCH_OUTPUT_DIR:-$(dirname "$output")}"

if command -v log_info >/dev/null 2>&1; then
  log_info "Generating benchmark report: $output"
fi

# Determine RUN_ID if not provided (latest by timestamp)
if [ -z "$run_id_env" ]; then
  run_id_env=$(awk -F, '
    function norm(h) {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", h)
      return tolower(h)
    }
    BEGIN {
      alias["git_sha"]="run_id"
    }
    NR==1 {
      for (i=1;i<=NF;i++) {
        h=norm($i)
        name=(h in alias)?alias[h]:h
        col[name]=i
      }
      if (!col["run_id"] || !col["timestamp"]) {
        print "error: missing required columns for run_id selection (run_id,timestamp)" > "/dev/stderr"
        exit 2
      }
      next
    }
    {
      rid=$(col["run_id"])
      ts=$(col["timestamp"])
      if (rid=="" || ts=="") next
      if (max_ts=="" || ts>max_ts) {
        max_ts=ts
        max_run=rid
      }
    }
    END {
      if (max_run=="") {
        print "error: unable to determine run_id from input" > "/dev/stderr"
        exit 3
      }
      print max_run
    }
  ' "$input")
fi

out_dir=$(dirname "$output")
mkdir -p "$out_dir"

gen_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Extract proxy versions from docker-compose.yaml if available
envoy_ver=""
haproxy_ver=""
nginx_ver=""
if [ -f "bench/docker-compose.yaml" ]; then
  envoy_ver=$(grep "image:.*envoy" bench/docker-compose.yaml | sed -E 's/.*:-([^}]+)}.*/\1/' | head -1)
  haproxy_ver=$(grep "image:.*haproxy" bench/docker-compose.yaml | sed -E 's/.*:-([^}]+)}.*/\1/' | head -1 | sed 's/-alpine//')
  nginx_ver=$(grep "image:.*nginx" bench/docker-compose.yaml | sed -E 's/.*:-([^}]+)}.*/\1/' | head -1 | sed 's/-alpine//')
fi

# Main AWK script for report generation
awk -F, -v run_id="$run_id_env" -v input="$input" -v gen_at="$gen_at" -v host_info="$host_info_content" \
    -v envoy_ver="$envoy_ver" -v haproxy_ver="$haproxy_ver" -v nginx_ver="$nginx_ver" '
  function norm(h) {
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", h)
    return tolower(h)
  }
  function is_num(x) {
    return x ~ /^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$/
  }
  function is_true(x, t) {
    t=tolower(x)
    return t=="1" || t=="true" || t=="yes" || t=="y"
  }
  function fmt_float(x, prec) {
    if (!is_num(x)) return "-"
    return sprintf("%." prec "f", x+0)
  }
  function fmt_int(x) {
    if (!is_num(x)) return "-"
    return sprintf("%.0f", x+0)
  }
  
  BEGIN {
    # Column mapping aliases
    alias["git_sha"]="run_id"
    alias["rps_iqr"]="rps_iqr"
    alias["p99_iqr"]="p99_iqr"
    alias["dropped"]="dropped"
    alias["errors"]="errors"
    alias["aggregate"]="aggregate"
    
    # Define required columns
    req_cols="run_id timestamp proxy case achieved_rps p99_ms errors dropped rps_iqr p99_iqr proxy_cpu peak_mem_mib aggregate cpu_model kernel"
    split(req_cols, req, " ")
    
    # Store versions
    p_ver["envoy"] = envoy_ver
    p_ver["haproxy"] = haproxy_ver
    p_ver["nginx"] = nginx_ver
  }

  # Parse Header
  NR==1 {
    for (i=1;i<=NF;i++) {
      h=norm($i)
      name=(h in alias)?alias[h]:h
      col[name]=i
    }
    next
  }

  # Parse Data Rows
  {
    # Filter by Run ID
    if ($(col["run_id"]) != run_id) next
    
    # Filter for aggregates ONLY (per strict instructions)
    if (!is_true($(col["aggregate"]))) next
    
    p = $(col["proxy"])
    c = $(col["case"])
    
    if (p == "" || c == "") next
    
    proxies[p] = 1
    
    # Store Raw Data using SUBSEP
    data[p,c,"achieved_rps"] = $(col["achieved_rps"])
    data[p,c,"p99_ms"] = $(col["p99_ms"])
    data[p,c,"p99_iqr"] = $(col["p99_iqr"])
    data[p,c,"proxy_cpu"] = $(col["proxy_cpu"])
    data[p,c,"peak_mem_mib"] = $(col["peak_mem_mib"])
    data[p,c,"errors"] = $(col["errors"])
    data[p,c,"dropped"] = $(col["dropped"])
    data[p,c,"rps_iqr"] = $(col["rps_iqr"])
    
    # Capture Run Metadata (from first row)
    if (run_ts == "") {
      run_ts = $(col["timestamp"])
      cpu_model = $(col["cpu_model"])
      sub(/ [0-9]+-Core Processor/, "", cpu_model) # Clean up "64-Core Processor" suffix
      kernel = $(col["kernel"])
    }
  }

  END {
    # --- CALCULATE DERIVED METRICS (First pass, unordered) ---
    for (p in proxies) {
      # Scoreboard Mappings
      sb[p,"max_rps"] = data[p,"throughput_short_1x","achieved_rps"]
      sb[p,"p99_ms"] = data[p,"latency_short_1x","p99_ms"]
      sb[p,"p99_iqr"] = data[p,"latency_extended_1x","p99_iqr"]
      
      # Derived: rps_per_cpu (latency_extended_1x)
      rps_ext = data[p,"latency_extended_1x","achieved_rps"]
      cpu_ext = data[p,"latency_extended_1x","proxy_cpu"]
      if (is_num(rps_ext) && is_num(cpu_ext) && cpu_ext > 0) {
        sb[p,"rps_per_cpu"] = rps_ext / cpu_ext
      } else {
        sb[p,"rps_per_cpu"] = 0
      }
      
      sb[p,"peak_mem_mib"] = data[p,"latency_extended_1x","peak_mem_mib"]
      
      # Sum Errors
      err_conc = data[p,"concurrency_short_1x","errors"]
      err_churn = data[p,"churn_short_1x","errors"]
      sb[p,"errors"] = (is_num(err_conc)?err_conc:0) + (is_num(err_churn)?err_churn:0)
      
      sb[p,"reload_p99_ms"] = data[p,"reload_short_1x","p99_ms"]
      
      # Derived: Usable Ratio
      max_r = data[p,"throughput_short_1x","achieved_rps"]
      lat_r = data[p,"latency_short_1x","achieved_rps"]
      if (is_num(max_r) && is_num(lat_r) && lat_r > 0) {
        sb[p,"usable_ratio"] = max_r / lat_r
      } else {
        sb[p,"usable_ratio"] = 0
      }
      
      # Resource Cost Profile (all from latency_extended_1x)
      rc[p,"cpu"] = cpu_ext
      rc[p,"mem_mib"] = data[p,"latency_extended_1x","peak_mem_mib"]
      rc[p,"rps_per_cpu"] = sb[p,"rps_per_cpu"]
      mem_ext = data[p,"latency_extended_1x","peak_mem_mib"]
      if (is_num(rps_ext) && is_num(mem_ext) && mem_ext > 0) {
        rc[p,"rps_per_mib"] = rps_ext / mem_ext
      } else {
        rc[p,"rps_per_mib"] = 0
      }
    }

    # --- SORT PROXIES BY MAX_RPS DESCENDING ---
    n_proxies = 0
    for (p in proxies) p_list[++n_proxies] = p
    
    for (i=1; i<=n_proxies; i++) {
        for (j=i+1; j<=n_proxies; j++) {
            # Use computed max_rps for sorting
            val_i = sb[p_list[i],"max_rps"] + 0
            val_j = sb[p_list[j],"max_rps"] + 0
            # Descending order
            if (val_i < val_j) {
                tmp = p_list[i]
                p_list[i] = p_list[j]
                p_list[j] = tmp
            }
        }
    }
    
    # --- DETERMINE BEST VALUES FOR SYMBOLS ---
    
    best["max_rps"] = 0
    best["p99_ms"] = 1e9
    best["rps_per_cpu"] = 0
    best["peak_mem_mib"] = 1e9
    best["reload_p99_ms"] = 1e9
    
    best_rc["cpu"] = 1e9
    best_rc["mem_mib"] = 1e9
    best_rc["rps_per_cpu"] = 0
    best_rc["rps_per_mib"] = 0
    
    for (i=1; i<=n_proxies; i++) {
      p = p_list[i]
      
      # Scoreboard Bests
      v = sb[p,"max_rps"]; if(is_num(v) && v > best["max_rps"]) best["max_rps"] = v
      v = sb[p,"p99_ms"]; if(is_num(v) && v < best["p99_ms"]) best["p99_ms"] = v
      v = sb[p,"rps_per_cpu"]; if(is_num(v) && v > best["rps_per_cpu"]) best["rps_per_cpu"] = v
      v = sb[p,"peak_mem_mib"]; if(is_num(v) && v < best["peak_mem_mib"]) best["peak_mem_mib"] = v
      v = sb[p,"reload_p99_ms"]; if(is_num(v) && v < best["reload_p99_ms"]) best["reload_p99_ms"] = v
      
      # Resource Bests
      v = rc[p,"cpu"]; if(is_num(v) && v < best_rc["cpu"]) best_rc["cpu"] = v
      v = rc[p,"mem_mib"]; if(is_num(v) && v < best_rc["mem_mib"]) best_rc["mem_mib"] = v
      v = rc[p,"rps_per_cpu"]; if(is_num(v) && v > best_rc["rps_per_cpu"]) best_rc["rps_per_cpu"] = v
      v = rc[p,"rps_per_mib"]; if(is_num(v) && v > best_rc["rps_per_mib"]) best_rc["rps_per_mib"] = v
    }

    # --- GENERATE MARKDOWN ---
    
    # 1. Header
    print "# Benchmark Report"
    print ""
    print "---"
    print ""
    print "## Run Context"
    print ""
    
    # Proxies string (now sorted)
    p_str = ""
    for (i=1; i<=n_proxies; i++) {
      p = p_list[i]
      ver = ""
      if (p == "pavis") {
        ver = "@" substr(run_id, 1, 6)
      } else if (p_ver[p] != "") {
        ver = "@" p_ver[p]
      }
      p_str = p_str (i>1 ? " · " : "") "`" p ver "`"
    }
    
    print "**run**: `" substr(run_id, 1, 10) "` · **time**: `" run_ts "`  "
    print "**env**: `" cpu_model "` · `" kernel "`  "
    print "**proxies**: " p_str "  "
    print "**cases**: `throughput` / `latency(short, extended)` / `concurrency` / `churn` / `reload`  "
    print "**methodology**: [METHODOLOGY.md](https://github.com/fabian4/pavis/blob/main/docs/benchmark/METHODOLOGY.md) · [CASES.md](https://github.com/fabian4/pavis/blob/main/docs/benchmark/CASES.md)  "
    print "**raw data**: " input
    print ""
    print "---"
    print ""
    
    # 2. Performance Scoreboard
    print "## Performance Scoreboard"
    print ""
    print "> One row per proxy. Values reflect **primary-case signals only**."
    print ""
    print "| proxy | max_rps | p99_ms | p99_iqr | rps_per_cpu | peak_mem_mib | errors | reload_p99_ms | usable_ratio |"
    print "|------|--------:|-------:|--------:|------------:|-------------:|-------:|--------------:|-------------:| "
    
    for (i=1; i<=n_proxies; i++) {
      p = p_list[i]
      
      # Prepare Row Values
      s_max_rps = fmt_int(sb[p,"max_rps"])
      if (sb[p,"max_rps"] < best["max_rps"]) s_max_rps = s_max_rps " ↓"
      
      s_p99 = fmt_float(sb[p,"p99_ms"], 3)
      if (sb[p,"p99_ms"] > best["p99_ms"]) s_p99 = s_p99 " ↓"
      
      s_p99_iqr = fmt_float(sb[p,"p99_iqr"], 3)
      if (sb[p,"p99_iqr"] > 0.5) s_p99_iqr = s_p99_iqr " ⚠︎" # Heuristic: >0.5ms absolute
      
      s_rpc = fmt_float(sb[p,"rps_per_cpu"], 1)
      if (sb[p,"rps_per_cpu"] < best["rps_per_cpu"]) s_rpc = s_rpc " ↓"
      
      s_mem = fmt_float(sb[p,"peak_mem_mib"], 2)
      if (sb[p,"peak_mem_mib"] > 2 * best["peak_mem_mib"]) s_mem = s_mem " ⚠︎"
      else if (sb[p,"peak_mem_mib"] > best["peak_mem_mib"]) s_mem = s_mem " ↓"
      
      err_val = sb[p,"errors"]
      s_err = fmt_int(err_val)
      if (err_val > 0) s_err = s_err " ⊗"
      
      s_reload = fmt_float(sb[p,"reload_p99_ms"], 3)
      if (sb[p,"reload_p99_ms"] > best["reload_p99_ms"]) s_reload = s_reload " ↓"
      
      s_ratio = fmt_float(sb[p,"usable_ratio"], 2)
      
      print "| " p " | " s_max_rps " | " s_p99 " | " s_p99_iqr " | " s_rpc " | " s_mem " | " s_err " | " s_reload " | " s_ratio " |"
    }
    
    print ""
    print "<small>"
    print "- ↓ relative to best-in-column  "
    print "- ⚠︎ notable cost or instability signal  "
    print "- ⊗ error observed (invalid for baseline use)  "
    print "</small>"
    print ""
    print "---"
    print ""

    # 3. Resource Cost Profile
    print "## Resource Cost Profile"
    print ""
    print "> Cost projection at ~10k sustained RPS (open-loop)."
    print ""
    print "| proxy | cpu | mem_mib | rps_per_cpu | rps_per_mib |"
    print "|------|----:|--------:|------------:|------------:| "
    
    for (i=1; i<=n_proxies; i++) {
      p = p_list[i]
      
      s_cpu = fmt_float(rc[p,"cpu"], 2)
      if (rc[p,"cpu"] > best_rc["cpu"]) s_cpu = s_cpu " ↓"
      
      s_mem = fmt_float(rc[p,"mem_mib"], 2)
      if (rc[p,"mem_mib"] > 2 * best_rc["mem_mib"]) s_mem = s_mem " ⚠︎"
      else if (rc[p,"mem_mib"] > best_rc["mem_mib"]) s_mem = s_mem " ↓"
      
      s_rpc = fmt_float(rc[p,"rps_per_cpu"], 1)
      if (rc[p,"rps_per_cpu"] < best_rc["rps_per_cpu"]) s_rpc = s_rpc " ↓"
      
      s_rpm = fmt_float(rc[p,"rps_per_mib"], 1)
      if (rc[p,"rps_per_mib"] < best_rc["rps_per_mib"]) s_rpm = s_rpm " ↓"
      
      print "| " p " | " s_cpu " | " s_mem " | " s_rpc " | " s_rpm " |"
    }
    
    print ""
    print "<small>"
    print "Higher `rps_per_cpu` / `rps_per_mib` indicates better efficiency."
    print "⚠︎ highlights dominant cost drivers rather than outright failure."
    print "</small>"
    print ""
    print "---"
    print ""

    # 4. Stability Appendix
    print "## Stability Appendix"
    print ""
    print "### latency_extended_1x (5 runs)"
    print ""
    print "| proxy | rps_med | rps_iqr | p99_ms | p99_iqr | dropped |"
    print "|------|--------:|--------:|-------:|--------:|--------:| "
    
    for (i=1; i<=n_proxies; i++) {
      p = p_list[i]
      c = "latency_extended_1x"
      
      d_rps = fmt_float(data[p,c,"achieved_rps"], 1)
      d_rps_iqr = fmt_float(data[p,c,"rps_iqr"], 2)
      d_p99 = fmt_float(data[p,c,"p99_ms"], 3)
      
      d_p99_iqr_val = data[p,c,"p99_iqr"]
      d_p99_iqr = fmt_float(d_p99_iqr_val, 3)
      if (is_num(d_p99_iqr_val) && d_p99_iqr_val > 0.5) d_p99_iqr = d_p99_iqr " ⚠︎"
      
      d_dropped = fmt_int(data[p,c,"dropped"])
      
      print "| " p " | " d_rps " | " d_rps_iqr " | " d_p99 " | " d_p99_iqr " | " d_dropped " |"
    }
    
    print ""
    print "<small>"
    print "Appendix data is intended for regression and variance inspection, not ranking."
    print "</small>"
    print ""
    print "---"
    print ""
    
    # 5. Raw Metrics Reference
    print "## Raw Metrics Reference"
    print ""
    print "All results are derived from `" input "`."
    print ""
    print "Key fields:  "
    print "`achieved_rps` (throughput) · `p99_ms` (SLA latency) · `p99_iqr` (tail variance) ·  "
    print "`proxy_cpu` (CPU cost) · `peak_mem_mib` (memory cost) ·  "
    print "`errors` (stress-only failures) · `dropped` (open-loop saturation indicator)"
  }
' "$input" > "$output"

echo "wrote: $output"