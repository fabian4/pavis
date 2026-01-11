#!/usr/bin/env bash
set -euo pipefail

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

tmp_out=$(mktemp "$out_dir/report.XXXXXX")
override_file=$(mktemp "$out_dir/report_overrides.XXXXXX")
trap 'rm -f "$tmp_out" "$override_file"' EXIT

gen_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

RUN_ID="$run_id_env" INPUT="$input" python3 - <<'PY' > "$override_file"
import csv
import glob
import json
import os
import pathlib
import sys

run_id = os.environ.get("RUN_ID", "").strip()
if not run_id:
    run_id = None

summary_path = os.environ.get("INPUT", "bench/output/summary.csv")
rows = []
with open(summary_path, newline="") as f:
    reader = csv.DictReader(f)
    for row in reader:
        rid = row.get("run_id") or row.get("git_sha")
        if run_id and rid != run_id:
            continue
        rows.append(row)

openloop_keys = {}
iter_sums = {}
iter_counts = {}
agg_runs = {}

for row in rows:
    typ = (row.get("type") or "").lower()
    if not typ.startswith("loadgen"):
        continue
    proxy = row.get("proxy") or ""
    case = row.get("case") or ""
    if not proxy or not case:
        continue
    key = (proxy, case)
    openloop_keys[key] = True
    if (row.get("aggregate") or "") in ("1", "true", "True", "TRUE", "yes", "YES"):
        runs = row.get("runs") or ""
        if runs:
            try:
                agg_runs[key] = int(float(runs))
            except ValueError:
                pass
        continue
    iter_counts[key] = iter_counts.get(key, 0) + 1
    dropped = row.get("dropped") or ""
    errors = row.get("errors") or ""
    if dropped:
        iter_sums[key] = iter_sums.get(key, (0, 0))
        iter_sums[key] = (iter_sums[key][0] + int(float(dropped)), iter_sums[key][1])
    if errors:
        iter_sums[key] = iter_sums.get(key, (0, 0))
        iter_sums[key] = (iter_sums[key][0], iter_sums[key][1] + int(float(errors)))

def iter_sum(key):
    return iter_sums.get(key, (0, 0))

for key in sorted(openloop_keys.keys()):
    runs = agg_runs.get(key, iter_counts.get(key, 0))
    if runs <= 1:
        continue
    proxy, case = key
    run_paths = sorted(glob.glob(f"bench/output/{proxy}/{case}/run_*/result.json"))
    if not run_paths:
        print(f"error: missing result.json runs for {proxy} {case}", file=sys.stderr)
        sys.exit(10)
    dropped_sum = 0
    errors_sum = 0
    for path in run_paths:
        data = json.loads(pathlib.Path(path).read_text())
        dropped_sum += int(data.get("dropped", 0) or 0)
        errors_sum += int(data.get("errors", 0) or 0)
    csv_dropped, csv_errors = iter_sum(key)
    status = "ok" if (dropped_sum == csv_dropped and errors_sum == csv_errors) else "override"
    print(f"validated {case} {proxy}: dropped sum = {dropped_sum} ({len(run_paths)} runs) [{status}]", file=sys.stderr)
    print(f"{proxy},{case},{len(run_paths)},{dropped_sum},{errors_sum}")
PY

awk -F, -v run_id="$run_id_env" -v input="$input" -v gen_at="$gen_at" -v overrides="$override_file" '
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
  function is_wrk(t, tl) {
    tl=tolower(t)
    return (tl ~ /^wrk/)
  }
  function is_open_loop(t, tl) {
    tl=tolower(t)
    return (tl ~ /^loadgen/)
  }
  function add_value(metric, key, val) {
    if (!is_num(val)) return
    n=++count[metric SUBSEP key]
    vals[metric SUBSEP key SUBSEP n]=val+0
  }
  function sum_add(metric, key, val) {
    if (!is_num(val)) return
    sums[metric SUBSEP key]+=val+0
    sum_count[metric SUBSEP key]++
  }
  function first_non_empty(metric, key, val) {
    if (val=="" || first[metric SUBSEP key] != "") return
    first[metric SUBSEP key]=val
  }
  function sort_numeric(a, n,    i,j,tmp) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        if (a[i] > a[j]) {
          tmp=a[i]; a[i]=a[j]; a[j]=tmp
        }
      }
    }
  }
  function key_float(x) {
    return sprintf("%.9f", x+0)
  }
  function rank_map_asc(values, n,    i,rank,k,prev) {
    rank=1
    prev=""
    for (i=1;i<=n;i++) {
      k=key_float(values[i])
      if (k!=prev) {
        rank_map[k]=rank
        rank++
        prev=k
      }
    }
  }
  function rank_map_desc(values, n,    i,rank,k,prev) {
    rank=1
    prev=""
    for (i=n;i>=1;i--) {
      k=key_float(values[i])
      if (k!=prev) {
        rank_map[k]=rank
        rank++
        prev=k
      }
    }
  }
  function median(metric, key,    n,i,tmp,med) {
    n=count[metric SUBSEP key]
    if (n<=0) return ""
    delete tmp
    for (i=1;i<=n;i++) tmp[i]=vals[metric SUBSEP key SUBSEP i]+0
    sort_numeric(tmp, n)
    if (n%2==1) med=tmp[(n+1)/2]
    else med=(tmp[n/2]+tmp[n/2+1])/2
    return med
  }
  function iqr(metric, key,    n,i,tmp,q1,q3,idx1,idx3) {
    n=count[metric SUBSEP key]
    if (n<=1) return ""
    delete tmp
    for (i=1;i<=n;i++) tmp[i]=vals[metric SUBSEP key SUBSEP i]+0
    sort_numeric(tmp, n)
    idx1=int(n/4+0.5); if (idx1<1) idx1=1
    idx3=int(3*n/4+0.5); if (idx3<1) idx3=1
    if (idx3>n) idx3=n
    q1=tmp[idx1]
    q3=tmp[idx3]
    return q3-q1
  }
  function fmt_float(x) {
    if (!is_num(x)) return "-"
    return sprintf("%.3f", x+0)
  }
  function fmt_int(x) {
    if (!is_num(x)) return "-"
    return sprintf("%.0f", x+0)
  }
  function fmt_text(x) {
    return (x=="" ? "-" : x)
  }
  function get_type(key) {
    if (agg_type[key] != "") return agg_type[key]
    if (iter_type[key] != "") return iter_type[key]
    return ""
  }
  function load_overrides(   line, parts, proxy, cas, key) {
    while ((getline line < overrides) > 0) {
      split(line, parts, ",")
      proxy=parts[1]
      cas=parts[2]
      key=proxy SUBSEP cas
      override_runs[key]=parts[3]
      override_dropped[key]=parts[4]
      override_errors[key]=parts[5]
    }
    close(overrides)
  }
  function sum_value(metric, key,    c) {
    c=sum_count[metric SUBSEP key]
    if (c>0) return sums[metric SUBSEP key]
    return ""
  }
  function openloop_value(metric, key,    c, agg) {
    if (metric=="dropped" && override_dropped[key] != "") return override_dropped[key]
    if (metric=="errors" && override_errors[key] != "") return override_errors[key]
    c=sum_count[metric SUBSEP key]
    if (c>0) return sums[metric SUBSEP key]
    if (agg_has[key]) {
      agg=agg_val[metric SUBSEP key]
      if (is_num(agg)) return agg
    }
    return ""
  }
  function value_for(metric, key, val) {
    if (metric=="errors" || metric=="dropped") {
      if (sum_count[metric SUBSEP key] > 0) return sum_value(metric, key)
      if (agg_has[key]) return agg_val[metric SUBSEP key]
      return ""
    }
    if (agg_has[key]) {
      val=agg_val[metric SUBSEP key]
      if ((metric=="rps_iqr" || metric=="p99_iqr") && !is_num(val)) return ""
      return val
    }
    if (metric=="target_rps") return first[metric SUBSEP key]
    if (metric=="rps_iqr") return iqr("achieved_rps", key)
    if (metric=="p99_iqr") return iqr("p99_ms", key)
    return median(metric, key)
  }
  function maybe_dropped(key, val, typ) {
    if (!is_open_loop(typ)) return "-"
    val=openloop_value("dropped", key)
    if (!is_num(val)) return "-"
    return fmt_int(val)
  }
  function maybe_target(key, val, typ) {
    if (!is_open_loop(typ)) return "-"
    if (!is_num(val)) return "-"
    return fmt_float(val)
  }
  function maybe_errors(key, val, typ, is_stability) {
    if (is_open_loop(typ)) {
      val=openloop_value("errors", key)
      if (!is_num(val)) return "-"
      return fmt_int(val)
    }
    if (is_wrk(typ)) return "-"
    if (is_num(val)) return fmt_int(val)
    if (is_stability) return "0"
    return "-"
  }
  function saturated_for(key, typ, dropped) {
    if (!is_open_loop(typ)) return "-"
    dropped=openloop_value("dropped", key)
    if (is_num(dropped) && dropped > 0) return "true"
    return "false"
  }
  function warn_backend_cpu(key, backend, proxy,    parts) {
    if (!is_num(backend) || !is_num(proxy)) return
    if (proxy+0 <= 0) return
    if ((backend+0) > ((proxy+0) * 2)) {
      split(key, parts, SUBSEP)
      print "warning: backend_cpu exceeds proxy_cpu by >2x for " parts[1] " " parts[2] > "/dev/stderr"
    }
  }
  function nearly_equal(a, b, eps) {
    if (!is_num(a) || !is_num(b)) return 0
    return ((a+0)-(b+0) < eps && (b+0)-(a+0) < eps)
  }
  function sort_alpha(list, n,    i,j,tmp) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        if (list[i] > list[j]) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function sort_proxies(list, n,    i,j,tmp,r1,r2) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        r1=proxy_rank[list[i]]; r2=proxy_rank[list[j]]
        if (r1==0) r1=999
        if (r2==0) r2=999
        if (r1 > r2 || (r1==r2 && list[i] > list[j])) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function sort_cases(list, n,    i,j,tmp,r1,r2) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        r1=case_rank[list[i]]; r2=case_rank[list[j]]
        if (r1==0) r1=999
        if (r2==0) r2=999
        if (r1 > r2 || (r1==r2 && list[i] > list[j])) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function join_sorted(set, sep,    n,i,out,k,list) {
    n=0
    for (k in set) list[++n]=k
    sort_alpha(list, n)
    out=""
    for (i=1;i<=n;i++) {
      out = out (i==1 ? "" : sep) list[i]
    }
    return out
  }
  function sort_proxy_case(list, n,    i,j,tmp,a,b) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        split(list[i], a, SUBSEP)
        split(list[j], b, SUBSEP)
        r1=proxy_rank[a[1]]; r2=proxy_rank[b[1]]
        if (r1==0) r1=999
        if (r2==0) r2=999
        c1=case_rank[a[2]]; c2=case_rank[b[2]]
        if (c1==0) c1=999
        if (c2==0) c2=999
        if (r1 > r2 || (r1==r2 && (c1 > c2 || (c1==c2 && a[2] > b[2])))) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function sort_case_proxy(list, n,    i,j,tmp,a,b,c1,c2,r1,r2) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        split(list[i], a, SUBSEP)
        split(list[j], b, SUBSEP)
        c1=case_rank[a[2]]; c2=case_rank[b[2]]
        if (c1==0) c1=999
        if (c2==0) c2=999
        r1=proxy_rank[a[1]]; r2=proxy_rank[b[1]]
        if (r1==0) r1=999
        if (r2==0) r2=999
        if (c1 > c2 || (c1==c2 && (r1 > r2 || (r1==r2 && a[1] > b[1])))) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function sort_p99(list, n,    i,j,tmp,n1,n2) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        n1=is_num(p99_by_proxy[list[i]])?p99_by_proxy[list[i]]+0:1e18
        n2=is_num(p99_by_proxy[list[j]])?p99_by_proxy[list[j]]+0:1e18
        if (n1 > n2 || (n1==n2 && list[i] > list[j])) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function sort_desc(list, n,    i,j,tmp,n1,n2) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        n1=is_num(throughput[list[i]])?throughput[list[i]]+0:-1
        n2=is_num(throughput[list[j]])?throughput[list[j]]+0:-1
        if (n1 < n2 || (n1==n2 && list[i] > list[j])) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  function sort_case_rps(list, n,    i,j,tmp,a,b,r1,r2) {
    for (i=1;i<=n;i++) {
      for (j=i+1;j<=n;j++) {
        split(list[i], a, SUBSEP)
        split(list[j], b, SUBSEP)
        r1=is_num(eff_val[list[i]])?eff_val[list[i]]+0:-1
        r2=is_num(eff_val[list[j]])?eff_val[list[j]]+0:-1
        c1=case_rank[a[1]]; c2=case_rank[b[1]]
        if (c1==0) c1=999
        if (c2==0) c2=999
        if (c1 > c2 || (c1==c2 && (r1 < r2 || (r1==r2 && a[2] > b[2])))) {
          tmp=list[i]; list[i]=list[j]; list[j]=tmp
        }
      }
    }
  }
  BEGIN {
    OFS=" | "
    alias["git_sha"]="run_id"
    alias["workload"]="case"
    alias["case_type"]="type"
    alias["rps"]="achieved_rps"
    alias["peak_cpu_pct"]="proxy_cpu"
    alias["avg_cpu_pct"]="proxy_cpu"
    alias["peak_backend_cpu_pct"]="backend_cpu"
    alias["avg_backend_cpu_pct"]="backend_cpu"
    alias["mem_peak_mib"]="peak_mem_mib"
    alias["target_rate"]="target_rps"
    alias["ts"]="timestamp"
    alias["is_aggregate"]="aggregate"

    proxy_rank["envoy"]=1
    proxy_rank["haproxy"]=2
    proxy_rank["nginx"]=3
    proxy_rank["pavis"]=4
    case_rank["churn_short_1x"]=1
    case_rank["concurrency_short_1x"]=2
    case_rank["latency_extended_1x"]=3
    case_rank["latency_short_1x"]=4
    case_rank["reload_short_1x"]=5
    case_rank["throughput_short_1x"]=6

    if (overrides != "") load_overrides()

    req[1]="proxy"
    req[2]="case"
    req[3]="type"
    req[4]="runs"
    req[5]="achieved_rps"
    req[6]="p50_ms"
    req[7]="p90_ms"
    req[8]="p99_ms"
    req[9]="errors"
    req[10]="dropped"
    req[11]="rps_iqr"
    req[12]="p99_iqr"
    req[13]="backend_cpu"
    req[14]="proxy_cpu"
    req[15]="peak_mem_mib"
    req[16]="target_rps"
    req[17]="timestamp"
    req[18]="run_id"
    req[19]="aggregate"
  }
  NR==1 {
    for (i=1;i<=NF;i++) {
      h=norm($i)
      name=(h in alias)?alias[h]:h
      col[name]=i
    }
    for (i=1;i in req;i++) {
      if (!col[req[i]]) {
        print "error: missing required column: " req[i] > "/dev/stderr"
        exit 2
      }
    }
    next
  }
  {
    rid=$(col["run_id"])
    if (rid != run_id) next
    found=1

    proxy=$(col["proxy"])
    cas=$(col["case"])
    if (proxy=="" || cas=="") next

    key=proxy SUBSEP cas
    keys[key]=1
    proxies[proxy]=1
    cases[cas]=1

    typ=$(col["type"])
    if (typ!="" && iter_type[key]=="") iter_type[key]=typ

    agg=is_true($(col["aggregate"]))
    if (agg) {
      # Aggregate row takes precedence; fallback to iteration aggregation only if missing.
      agg_has[key]=1
      if (typ!="") agg_type[key]=typ
      agg_runs[key]=$(col["runs"])
      agg_val["achieved_rps" SUBSEP key]=$(col["achieved_rps"])
      agg_val["p50_ms" SUBSEP key]=$(col["p50_ms"])
      agg_val["p90_ms" SUBSEP key]=$(col["p90_ms"])
      agg_val["p99_ms" SUBSEP key]=$(col["p99_ms"])
      agg_val["errors" SUBSEP key]=$(col["errors"])
      agg_val["dropped" SUBSEP key]=$(col["dropped"])
      agg_val["rps_iqr" SUBSEP key]=$(col["rps_iqr"])
      agg_val["p99_iqr" SUBSEP key]=$(col["p99_iqr"])
      agg_val["backend_cpu" SUBSEP key]=$(col["backend_cpu"])
      agg_val["proxy_cpu" SUBSEP key]=$(col["proxy_cpu"])
      agg_val["peak_mem_mib" SUBSEP key]=$(col["peak_mem_mib"])
      agg_val["target_rps" SUBSEP key]=$(col["target_rps"])
    } else {
      iter_runs[key]++
      add_value("achieved_rps", key, $(col["achieved_rps"]))
      add_value("p50_ms", key, $(col["p50_ms"]))
      add_value("p90_ms", key, $(col["p90_ms"]))
      add_value("p99_ms", key, $(col["p99_ms"]))
      add_value("backend_cpu", key, $(col["backend_cpu"]))
      add_value("proxy_cpu", key, $(col["proxy_cpu"]))
      add_value("peak_mem_mib", key, $(col["peak_mem_mib"]))
      sum_add("errors", key, $(col["errors"]))
      sum_add("dropped", key, $(col["dropped"]))
      first_non_empty("target_rps", key, $(col["target_rps"]))
    }
  }
  END {
    if (!found) {
      print "error: no rows for selected run_id: " run_id > "/dev/stderr"
      exit 4
    }

    for (key in keys) {
      typ=get_type(key)
      runs=agg_has[key] && is_num(agg_runs[key]) ? agg_runs[key] : iter_runs[key]
      if (is_open_loop(typ) && is_num(runs) && runs+0 > 1) {
        if (sum_count["dropped" SUBSEP key] <= 0 || sum_count["errors" SUBSEP key] <= 0) {
          split(key, parts, SUBSEP)
          print "error: missing iteration rows for open-loop multi-run " parts[1] " " parts[2] > "/dev/stderr"
          exit 7
        }
      }
      if (agg_has[key] && count["achieved_rps" SUBSEP key] > 0) {
        if (is_num(agg_val["achieved_rps" SUBSEP key]) && !nearly_equal(agg_val["achieved_rps" SUBSEP key], median("achieved_rps", key), 1e-3)) {
          split(key, parts, SUBSEP)
          print "error: achieved_rps aggregate mismatch for " parts[1] " " parts[2] > "/dev/stderr"
          exit 8
        }
        if (is_num(agg_val["p50_ms" SUBSEP key]) && !nearly_equal(agg_val["p50_ms" SUBSEP key], median("p50_ms", key), 1e-3)) {
          split(key, parts, SUBSEP)
          print "error: p50_ms aggregate mismatch for " parts[1] " " parts[2] > "/dev/stderr"
          exit 8
        }
        if (is_num(agg_val["p90_ms" SUBSEP key]) && !nearly_equal(agg_val["p90_ms" SUBSEP key], median("p90_ms", key), 1e-3)) {
          split(key, parts, SUBSEP)
          print "error: p90_ms aggregate mismatch for " parts[1] " " parts[2] > "/dev/stderr"
          exit 8
        }
        if (is_num(agg_val["p99_ms" SUBSEP key]) && !nearly_equal(agg_val["p99_ms" SUBSEP key], median("p99_ms", key), 1e-3)) {
          split(key, parts, SUBSEP)
          print "error: p99_ms aggregate mismatch for " parts[1] " " parts[2] > "/dev/stderr"
          exit 8
        }
        if (is_num(agg_val["rps_iqr" SUBSEP key]) && !nearly_equal(agg_val["rps_iqr" SUBSEP key], iqr("achieved_rps", key), 1e-3)) {
          split(key, parts, SUBSEP)
          print "error: rps_iqr aggregate mismatch for " parts[1] " " parts[2] > "/dev/stderr"
          exit 8
        }
        if (is_num(agg_val["p99_iqr" SUBSEP key]) && !nearly_equal(agg_val["p99_iqr" SUBSEP key], iqr("p99_ms", key), 1e-3)) {
          split(key, parts, SUBSEP)
          print "error: p99_iqr aggregate mismatch for " parts[1] " " parts[2] > "/dev/stderr"
          exit 8
        }
      }
    }

    print "# Key Report"
    print ""
    n=0
    for (p in proxies) p_list[++n]=p
    sort_proxies(p_list, n)
    p_out=""
    for (i=1;i<=n;i++) p_out = p_out (i==1 ? "" : ", ") p_list[i]

    n=0
    for (c in cases) c_list[++n]=c
    sort_cases(c_list, n)
    c_out=""
    for (i=1;i<=n;i++) c_out = c_out (i==1 ? "" : ", ") c_list[i]

    print "- generated_at: " gen_at
    print "- input: " input
    print "- run_id: " run_id
    print "- proxies: " p_out
    print "- cases: " c_out
    print ""
    print "## Interpretation Rules"
    print ""
    print "- wrk cases are closed-loop; dropped and saturation are not applicable."
    print "- loadgen cases are open-loop; dropped indicates saturation."
    print "- For loadgen multi-run cases, dropped/errors are SUM across all iterations."
    print "- Multi-run cases report median and IQR across runs."
    print "- Metrics are comparable only within the same case."
    print "- CPU/memory metrics are taken from summary.csv for all cases and are not comparable across case types."
    print ""

    print "## 📊 Per-Case Performance Summary"
    print ""
    print "proxy | case | achieved_rps | p50_ms | p90_ms | p99_ms | errors | dropped | backend_cpu | proxy_cpu | peak_mem_mib | target_rps"
    print "--- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | ---"
    n=0
    for (k in keys) key_list[++n]=k
    sort_proxy_case(key_list, n)
    for (i=1;i<=n;i++) {
      key=key_list[i]
      split(key, parts, SUBSEP)
      proxy=parts[1]
      cas=parts[2]
      typ=get_type(key)
      warn_backend_cpu(key, value_for("backend_cpu", key), value_for("proxy_cpu", key))
      dropped_out=maybe_dropped(key, value_for("dropped", key), typ)
      errors_out=maybe_errors(key, value_for("errors", key), typ, 0)
      if (is_open_loop(typ) && (dropped_out=="-" || errors_out=="-")) {
        print "error: open-loop missing dropped/errors for " proxy " " cas > "/dev/stderr"
        exit 5
      }
      if (is_wrk(typ) && dropped_out != "-") {
        print "error: wrk dropped must be '-' for " proxy " " cas > "/dev/stderr"
        exit 6
      }
      print proxy, cas,
        fmt_float(value_for("achieved_rps", key)),
        fmt_float(value_for("p50_ms", key)),
        fmt_float(value_for("p90_ms", key)),
        fmt_float(value_for("p99_ms", key)),
        errors_out,
        dropped_out,
        fmt_float(value_for("backend_cpu", key)),
        fmt_float(value_for("proxy_cpu", key)),
        fmt_float(value_for("peak_mem_mib", key)),
        maybe_target(key, value_for("target_rps", key), typ)
    }
    print ""

    print "## ⏱️ Latency Comparison (latency_short_1x)"
    print ""
    for (p in proxies) {
      key=p SUBSEP "latency_short_1x"
      if (!keys[key]) continue
      p99=value_for("p99_ms", key)
      p99_by_proxy[p]=p99
    }
    p99_count=0
    for (p in p99_by_proxy) p99_list[++p99_count]=p
    sort_p99(p99_list, p99_count)
    best_p99_proxy=""
    best_p99_val=""
    p99_n=0
    for (p in p99_by_proxy) {
      if (is_num(p99_by_proxy[p])) {
        p99_vals[++p99_n]=p99_by_proxy[p]+0
      }
    }
    if (p99_n>0) {
      sort_numeric(p99_vals, p99_n)
      delete rank_map
      rank_map_asc(p99_vals, p99_n)
      for (i=1;i<=p99_count;i++) {
        p=p99_list[i]
        if (is_num(p99_by_proxy[p])) {
          best_p99_proxy=p
          best_p99_val=p99_by_proxy[p]
          break
        }
      }
      if (best_p99_proxy!="") {
        print "- Fastest p99 latency: " best_p99_proxy " (" fmt_float(best_p99_val) " ms)"
        print ""
      }
    }
    print "proxy | achieved_rps | p50_ms | p90_ms | p99_ms | dropped | errors | backend_cpu | proxy_cpu | peak_mem_mib | saturated | p99_rank | is_best_p99"
    print "--- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | ---"
    for (i=1;i<=p99_count;i++) {
      p=p99_list[i]
      key=p SUBSEP "latency_short_1x"
      typ=get_type(key)
      if (is_num(p99_by_proxy[p])) {
        p99_key=key_float(p99_by_proxy[p])
        p99_rank=(p99_key in rank_map) ? rank_map[p99_key] : "-"
      } else {
        p99_rank="-"
      }
      is_best=(p99_rank==1 ? "true" : "false")
      print p,
        fmt_float(value_for("achieved_rps", key)),
        fmt_float(value_for("p50_ms", key)),
        fmt_float(value_for("p90_ms", key)),
        fmt_float(value_for("p99_ms", key)),
        maybe_dropped(key, value_for("dropped", key), typ),
        maybe_errors(key, value_for("errors", key), typ, 0),
        fmt_float(value_for("backend_cpu", key)),
        fmt_float(value_for("proxy_cpu", key)),
        fmt_float(value_for("peak_mem_mib", key)),
        saturated_for(key, typ),
        p99_rank,
        is_best
    }
    print ""

    print "## 🚀 Throughput & Stress Results"
    print ""
    for (p in proxies) {
      key=p SUBSEP "throughput_short_1x"
      t=value_for("achieved_rps", key)
      throughput[p]=t
    }
    th_n=0
    for (p in throughput) {
      if (is_num(throughput[p])) {
        th_vals[++th_n]=throughput[p]+0
      }
    }
    if (th_n>0) {
      sort_numeric(th_vals, th_n)
      delete rank_map
      rank_map_desc(th_vals, th_n)
    }
    n=0
    for (p in throughput) t_list[++n]=p
    sort_desc(t_list, n)
    if (n>0) {
      best_th_proxy=t_list[1]
      best_th_val=throughput[best_th_proxy]
      if (is_num(best_th_val)) {
        print "- Highest throughput: " best_th_proxy " (" fmt_float(best_th_val) " rps)"
        print ""
      }
    }
    print "proxy | throughput_short_1x | concurrency_short_1x | churn_short_1x | concurrency_errors | throughput_rank | is_top_throughput"
    print "--- | --- | --- | --- | --- | --- | ---"
    for (i=1;i<=n;i++) {
      p=t_list[i]
      th=value_for("achieved_rps", p SUBSEP "throughput_short_1x")
      conc=value_for("achieved_rps", p SUBSEP "concurrency_short_1x")
      churn=value_for("achieved_rps", p SUBSEP "churn_short_1x")
      # concurrency_errors uses summary.csv errors for wrk; values must be present.
      conc_err=value_for("errors", p SUBSEP "concurrency_short_1x")
      if (!is_num(conc_err)) {
        print "error: missing concurrency errors for " p > "/dev/stderr"
        exit 9
      }
      if (is_num(throughput[p])) {
        th_key=key_float(throughput[p])
        th_rank=(th_key in rank_map) ? rank_map[th_key] : "-"
      } else {
        th_rank="-"
      }
      is_top=(th_rank==1 ? "true" : "false")
      print p,
        fmt_float(th),
        fmt_float(conc),
        fmt_float(churn),
        fmt_int(conc_err),
        th_rank,
        is_top
    }
    print ""

    print "## 📉 Stability Across Iterations (Multi-run)"
    print ""
    print "Note: achieved_rps_median is the median across runs; dropped/errors are SUM across runs for loadgen cases."
    print ""
    best_p99_iqr=""
    best_p99_iqr_key=""
    for (key in keys) {
      runs=agg_has[key] && is_num(agg_runs[key]) ? agg_runs[key] : iter_runs[key]
      if (!is_num(runs) || runs+0 <= 1) continue
      stab_keys[key]=1
    }
    n=0
    for (k in stab_keys) s_list[++n]=k
    sort_case_proxy(s_list, n)
    for (i=1;i<=n;i++) {
      key=s_list[i]
      split(key, parts, SUBSEP)
      proxy=parts[1]
      cas=parts[2]
      runs=agg_has[key] && is_num(agg_runs[key]) ? agg_runs[key] : iter_runs[key]
      p99_iqr=value_for("p99_iqr", key)
      if (is_num(p99_iqr)) {
        if (best_p99_iqr=="" || p99_iqr+0 < best_p99_iqr+0) {
          best_p99_iqr=p99_iqr
          best_p99_iqr_key=proxy " " cas
        }
      }
    }
    if (best_p99_iqr_key!="") {
      print "- Lowest p99 IQR: " best_p99_iqr_key " (" fmt_float(best_p99_iqr) " ms)"
      print ""
    }
    print "proxy | case | runs | achieved_rps_median | rps_iqr | p99_ms | p99_iqr | dropped | errors"
    print "--- | --- | --- | --- | --- | --- | --- | --- | ---"
    for (i=1;i<=n;i++) {
      key=s_list[i]
      split(key, parts, SUBSEP)
      proxy=parts[1]
      cas=parts[2]
      typ=get_type(key)
      runs=agg_has[key] && is_num(agg_runs[key]) ? agg_runs[key] : iter_runs[key]
      print proxy, cas,
        fmt_int(runs),
        fmt_float(value_for("achieved_rps", key)),
        fmt_float(value_for("rps_iqr", key)),
        fmt_float(value_for("p99_ms", key)),
        fmt_float(value_for("p99_iqr", key)),
        maybe_dropped(key, value_for("dropped", key), typ),
        maybe_errors(key, value_for("errors", key), typ, 1)
    }
    print ""

    print "## ⚙️ Resource Efficiency (Open-loop)"
    print ""
    print "Note: Table E includes loadgen cases only."
    print ""
    open_case["latency_short_1x"]=1
    open_case["latency_extended_1x"]=1
    open_case["reload_short_1x"]=1
    for (key in keys) {
      split(key, parts, SUBSEP)
      proxy=parts[1]
      cas=parts[2]
      if (!open_case[cas]) continue
      typ=get_type(key)
      if (!is_open_loop(typ)) continue
      achieved=value_for("achieved_rps", key)
      cpu=value_for("proxy_cpu", key)
      mem=value_for("peak_mem_mib", key)
      if (!is_num(achieved) || !is_num(cpu) || !is_num(mem)) continue
      if (cpu+0 <= 0 || mem+0 <= 0) continue
      rps_cpu=(achieved+0)/(cpu+0)
      rps_mem=(achieved+0)/(mem+0)
      eff_key=cas SUBSEP proxy
      eff_keys[eff_key]=1
      eff_val[eff_key]=rps_cpu
      eff_ach[eff_key]=achieved
      eff_cpu[eff_key]=cpu
      eff_mem[eff_key]=mem
    }
    n=0
    for (k in eff_keys) e_list[++n]=k
    sort_case_rps(e_list, n)
    best_eff_key=""
    best_eff_val=""
    for (i=1;i<=n;i++) {
      k=e_list[i]
      if (best_eff_key=="" || eff_val[k]+0 > best_eff_val+0) {
        best_eff_key=k
        best_eff_val=eff_val[k]
      }
    }
    if (best_eff_key!="") {
      split(best_eff_key, parts, SUBSEP)
      print "- Highest rps_per_proxy_cpu: " parts[2] " " parts[1] " (" fmt_float(best_eff_val) ")"
      print ""
    }
    print "proxy | case | achieved_rps | proxy_cpu | peak_mem_mib | rps_per_proxy_cpu | rps_per_mib | rps_per_cpu_rank | rps_per_mib_rank"
    print "--- | --- | --- | --- | --- | --- | --- | --- | ---"
    cpu_n=0
    mib_n=0
    for (k in eff_keys) {
      eff_cpu_vals[++cpu_n]=eff_val[k]+0
      if (eff_mem[k]>0) {
        eff_mib_vals[++mib_n]=(eff_ach[k]/eff_mem[k])
      }
    }
    if (cpu_n>0) {
      sort_numeric(eff_cpu_vals, cpu_n)
      delete cpu_rank_map
      delete rank_map
      rank_map_desc(eff_cpu_vals, cpu_n)
      for (k in rank_map) cpu_rank_map[k]=rank_map[k]
    }
    if (mib_n>0) {
      sort_numeric(eff_mib_vals, mib_n)
      delete mib_rank_map
      delete rank_map
      rank_map_desc(eff_mib_vals, mib_n)
      for (k in rank_map) mib_rank_map[k]=rank_map[k]
    }
    for (i=1;i<=n;i++) {
      eff_key=e_list[i]
      split(eff_key, parts, SUBSEP)
      cas=parts[1]
      proxy=parts[2]
      achieved=eff_ach[eff_key]
      cpu=eff_cpu[eff_key]
      mem=eff_mem[eff_key]
      rps_cpu=eff_val[eff_key]
      rps_mem=(achieved+0)/(mem+0)
      cpu_rank=cpu_rank_map[key_float(rps_cpu)]
      if (cpu_rank=="") cpu_rank="-"
      mib_rank=mib_rank_map[key_float(rps_mem)]
      if (mib_rank=="") mib_rank="-"
      print proxy, cas,
        fmt_float(achieved),
        fmt_float(cpu),
        fmt_float(mem),
        fmt_float(rps_cpu),
        fmt_float(rps_mem),
        cpu_rank,
        mib_rank
    }
    print ""
    print "Metrics are comparable only within the same case."
  }
' "$input" > "$tmp_out"

mv -f "$tmp_out" "$output"
trap - EXIT

echo "wrote: $output"
