#!/usr/bin/env bash
set -euo pipefail

SUMMARY_CSV="${SUMMARY_CSV:-bench/output/standalone/summary.csv}"
REPORT_OUT="${REPORT_OUT:-bench/output/standalone/report.workstation.md}"

if [[ ! -f "$SUMMARY_CSV" ]]; then
  echo "error: summary csv not found: $SUMMARY_CSV" >&2
  exit 1
fi

awk '
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
function is_num(x) { return x ~ /^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$/ }
function fmt_int(x) { if (x=="" || !is_num(x)) return "—"; return sprintf("%.0f", x+0) }
function fmt3(x) { if (x=="" || !is_num(x)) return "—"; return sprintf("%.3f", x+0) }
function fmt2(x) { if (x=="" || !is_num(x)) return "—"; return sprintf("%.2f", x+0) }
function fmt1(x) { if (x=="" || !is_num(x)) return "—"; return sprintf("%.1f", x+0) }
function get(f, name) { return f[col[name]] }
function with_suffix(val, suffix) {
  if (val == "—" || suffix == "") return val
  return val " " suffix
}

BEGIN {
  proxy_order[1]="haproxy"
  proxy_order[2]="nginx"
  proxy_order[3]="pavis"
  proxy_order[4]="envoy"
  payload_order[1]="64B"
  payload_order[2]="4KiB"
}
NR==1 {
  n=csv_split($0, f)
  for (i=1; i<=n; i++) {
    col[trim(f[i])] = i
  }
  next
}
{
  n=csv_split($0, f)
  phase=get(f,"phase")
  bench_mode=get(f,"bench_mode")
  bench_profile=get(f,"bench_profile")
  aggregate=get(f,"aggregate")

  if (phase != "measure") next
  if (bench_mode != "standalone") next
  if (bench_profile != "workstation") next
  if (aggregate != "1") next

  proxy=get(f,"proxy")
  payload=get(f,"bench_payload_size")
  if (proxy == "" || payload == "") next

  payload_present[payload]=1
  proxy_present[proxy]=1
  row_count++

  if (!first_set) {
    first_set=1
    first_git_sha=get(f,"git_sha")
    first_ts=get(f,"timestamp")
    first_cpu=get(f,"cpu_model")
    first_kernel=get(f,"kernel")
  }

  case_name=get(f,"case")
  case_type=get(f,"type")

  if (case_name=="throughput_short_1x" && payload=="64B") {
    pref_git_sha=get(f,"git_sha")
    pref_ts=get(f,"timestamp")
    pref_cpu=get(f,"cpu_model")
    pref_kernel=get(f,"kernel")
  }

  key=proxy SUBSEP payload

  if (case_name=="throughput_short_1x" && case_type=="wrk-multi") {
    throughput_seen[key]=1
    max_rps[key]=get(f,"achieved_rps")
  }

  if (case_name=="latency_extended_1x" && case_type=="loadgen-multi") {
    latency_seen[key]=1
    p99_ms[key]=get(f,"p99_ms")
    p99_iqr[key]=get(f,"p99_iqr")
    errors[key]=get(f,"errors")
    dropped[key]=get(f,"dropped")
    cpu_avg[key]=get(f,"proxy_cpu_avg")
    mem_peak[key]=get(f,"proxy_mem_peak_mib")
    rps_med[key]=get(f,"achieved_rps")
    rps_iqr[key]=get(f,"rps_iqr")
  }
}
END {
  if (row_count == 0) {
    print "error: no standalone/workstation rows found in input" > "/dev/stderr"
    exit 2
  }

  git_sha = pref_git_sha != "" ? pref_git_sha : first_git_sha
  ts = pref_ts != "" ? pref_ts : first_ts
  cpu = pref_cpu != "" ? pref_cpu : first_cpu
  kernel = pref_kernel != "" ? pref_kernel : first_kernel

  run_id = git_sha != "" ? substr(git_sha,1,8) : "unknown"

  print "# Benchmark Report"
  print ""
  print "> Intended for health checks, regression detection, and multi-proxy trend comparison."
  print ""
  print "---"
  print ""
  print "## Run Context"
  print ""
  print "- **run**: `" run_id "` · **time**: `" ts "`"
  print "- **env**: `" cpu "` · `" kernel "`"
  print "- **mode**: `workstation / standalone`"
  print "- **payloads**: `64B`, `4KiB`"
  print "- **proxies**: `haproxy@unknown` · `nginx@unknown` · `pavis@" run_id "` · `envoy@unknown`"
  print "- **cases**: `throughput` · `latency(short/extended)` · `concurrency` · `churn` · `reload`"
  print "- **docs**: docs/benchmark/METHODOLOGY.md · docs/benchmark/CASES_STANDALONE.md"
  print "- **data**: bench/output/standalone/summary.csv"
  print "---"
  print ""
  print "## 1. Primary Performance Scoreboard"
  print ""
  print "> Primary signals only.  "
  print "> Derived from throughput_short_1x and latency_extended_1x.  "
  print "> Higher is better unless noted."
  print ""
  print "| proxy   | payload | max_rps | p99_ms | p99_iqr | verdict |"
  print "|--------:|:--------|--------:|-------:|--------:|:--------|"

  for (pi=1; pi<=2; pi++) {
    payload=payload_order[pi]
    if (!payload_present[payload]) continue
    best_max_rps[payload]=""
    best_p99[payload]=""
    best_p99_iqr[payload]=""
    for (pj=1; pj<=4; pj++) {
      proxy=proxy_order[pj]
      key=proxy SUBSEP payload
      if (throughput_seen[key] && is_num(max_rps[key])) {
        if (best_max_rps[payload]=="" || max_rps[key]+0 > best_max_rps[payload]+0) best_max_rps[payload]=max_rps[key]
      }
      if (latency_seen[key] && is_num(p99_ms[key])) {
        if (best_p99[payload]=="" || p99_ms[key]+0 < best_p99[payload]+0) best_p99[payload]=p99_ms[key]
      }
      if (latency_seen[key] && is_num(p99_iqr[key])) {
        if (best_p99_iqr[payload]=="" || p99_iqr[key]+0 < best_p99_iqr[payload]+0) best_p99_iqr[payload]=p99_iqr[key]
      }
    }
  }

  payload_blocks=0
  for (pi=1; pi<=2; pi++) {
    payload=payload_order[pi]
    if (!payload_present[payload]) continue
    payload_blocks++
    if (payload_blocks > 1) {
      print "| — | — | — | — | — | — |"
    }
    for (pj=1; pj<=4; pj++) {
      proxy=proxy_order[pj]
      key=proxy SUBSEP payload

      t_seen = throughput_seen[key]
      l_seen = latency_seen[key]

      maxr = t_seen ? fmt_int(max_rps[key]) : "—"
      p99 = l_seen ? fmt3(p99_ms[key]) : "—"
      p99iqr = l_seen ? fmt3(p99_iqr[key]) : "—"

      if (maxr != "—" && best_max_rps[payload] != "" && max_rps[key]+0 < best_max_rps[payload]+0) {
        maxr=with_suffix(maxr, "↓")
      }
      if (p99 != "—" && best_p99[payload] != "" && p99_ms[key]+0 > best_p99[payload]+0) {
        p99=with_suffix(p99, "↑")
      }
      if (p99iqr != "—" && best_p99_iqr[payload] != "" && p99_iqr[key]+0 > best_p99_iqr[payload]+0) {
        p99iqr=with_suffix(p99iqr, "↑")
      }

      verdict = "PASS"
      if (!t_seen || !l_seen) {
        verdict = "FAIL"
      } else {
        e = errors[key]
        d = dropped[key]
        if ((e != "" && is_num(e) && e+0 > 0) || (d != "" && is_num(d) && d+0 > 0)) {
          verdict = "WARN"
        }
      }

      printf "| %-7s | %-7s | %7s | %6s | %7s | %-7s |\n", proxy, payload, maxr, p99, p99iqr, verdict
    }
  }

  print ""
  print "---"
  print ""
  print "## 2. Resource Cost & Efficiency"
  print ""
  print "> Cost projection at ~10k sustained RPS (open-loop).  "
  print "> Higher rps_per_cpu / rps_per_mib indicates better efficiency."
  print ""
  print "| proxy   | payload | cpu_avg | mem_peak_mib | rps_per_cpu | rps_per_mib |"
  print "|--------:|:--------|--------:|-------------:|------------:|------------:|"

  for (pi=1; pi<=2; pi++) {
    payload=payload_order[pi]
    if (!payload_present[payload]) continue
    best_cpu[payload]=""
    best_mem[payload]=""
    best_rps_cpu[payload]=""
    best_rps_mib[payload]=""
    for (pj=1; pj<=4; pj++) {
      proxy=proxy_order[pj]
      key=proxy SUBSEP payload
      if (!latency_seen[key]) continue
      cpu=cpu_avg[key]
      mem=mem_peak[key]
      targ=rps_med[key]
      if (is_num(cpu)) {
        if (best_cpu[payload]=="" || cpu+0 < best_cpu[payload]+0) best_cpu[payload]=cpu
      }
      if (is_num(mem)) {
        if (best_mem[payload]=="" || mem+0 < best_mem[payload]+0) best_mem[payload]=mem
      }
      if (is_num(targ) && is_num(cpu) && cpu+0 > 0) {
        rps_cpu_val=(targ+0)/(cpu+0)
        if (best_rps_cpu[payload]=="" || rps_cpu_val > best_rps_cpu[payload]+0) best_rps_cpu[payload]=rps_cpu_val
      }
      if (is_num(targ) && is_num(mem) && mem+0 > 0) {
        rps_mib_val=(targ+0)/(mem+0)
        if (best_rps_mib[payload]=="" || rps_mib_val > best_rps_mib[payload]+0) best_rps_mib[payload]=rps_mib_val
      }
    }
  }

  payload_blocks=0
  for (pi=1; pi<=2; pi++) {
    payload=payload_order[pi]
    if (!payload_present[payload]) continue
    payload_blocks++
    if (payload_blocks > 1) {
      print "| — | — | — | — | — | — |"
    }
    for (pj=1; pj<=4; pj++) {
      proxy=proxy_order[pj]
      key=proxy SUBSEP payload

      if (latency_seen[key]) {
        cpu=cpu_avg[key]
        mem=mem_peak[key]
        targ=rps_med[key]
        cpu_out=fmt2(cpu)
        mem_out=fmt2(mem)
        if (is_num(targ) && is_num(cpu) && cpu+0 > 0) {
          rps_cpu_val=(targ+0)/(cpu+0)
          rps_cpu=sprintf("%.0f", rps_cpu_val)
        } else {
          rps_cpu_val=""
          rps_cpu="—"
        }
        if (is_num(targ) && is_num(mem) && mem+0 > 0) {
          rps_mib_val=(targ+0)/(mem+0)
          rps_mib=sprintf("%.0f", rps_mib_val)
        } else {
          rps_mib_val=""
          rps_mib="—"
        }

        if (cpu_out != "—" && best_cpu[payload] != "" && cpu+0 > best_cpu[payload]+0) {
          cpu_out=with_suffix(cpu_out, "↑")
        }
        if (mem_out != "—" && best_mem[payload] != "") {
          if (mem+0 >= (best_mem[payload]+0)*1.5) {
            mem_out=with_suffix(mem_out, "⚠︎")
          } else if (mem+0 > best_mem[payload]+0) {
            mem_out=with_suffix(mem_out, "↑")
          }
        }
        if (rps_cpu != "—" && best_rps_cpu[payload] != "" && rps_cpu_val != "" && rps_cpu_val < best_rps_cpu[payload]+0) {
          rps_cpu=with_suffix(rps_cpu, "↓")
        }
        if (rps_mib != "—" && best_rps_mib[payload] != "" && rps_mib_val != "" && rps_mib_val < best_rps_mib[payload]+0) {
          rps_mib=with_suffix(rps_mib, "↓")
        }
      } else {
        cpu_out="—"
        mem_out="—"
        rps_cpu="—"
        rps_mib="—"
      }

      printf "| %-7s | %-7s | %7s | %12s | %11s | %11s |\n", proxy, payload, cpu_out, mem_out, rps_cpu, rps_mib
    }
  }

  print ""
  print "---"
  print ""
  print "## 3. Stability & Tail Variance"
  print ""
  print "> Stability signals from latency_extended_1x (5 runs).  "
  print "> Used for regression detection, not ranking."
  print ""
  print "| proxy   | payload | rps_med | rps_iqr | p99_ms | p99_iqr | dropped |"
  print "|--------:|:--------|--------:|--------:|-------:|--------:|--------:|"

  for (pi=1; pi<=2; pi++) {
    payload=payload_order[pi]
    if (!payload_present[payload]) continue
    best_rps_med[payload]=""
    best_rps_iqr[payload]=""
    best_p99_t3[payload]=""
    best_p99_iqr_t3[payload]=""
    best_dropped[payload]=""
    for (pj=1; pj<=4; pj++) {
      proxy=proxy_order[pj]
      key=proxy SUBSEP payload
      if (!latency_seen[key]) continue
      if (is_num(rps_med[key])) {
        if (best_rps_med[payload]=="" || rps_med[key]+0 > best_rps_med[payload]+0) best_rps_med[payload]=rps_med[key]
      }
      if (is_num(rps_iqr[key])) {
        if (best_rps_iqr[payload]=="" || rps_iqr[key]+0 < best_rps_iqr[payload]+0) best_rps_iqr[payload]=rps_iqr[key]
      }
      if (is_num(p99_ms[key])) {
        if (best_p99_t3[payload]=="" || p99_ms[key]+0 < best_p99_t3[payload]+0) best_p99_t3[payload]=p99_ms[key]
      }
      if (is_num(p99_iqr[key])) {
        if (best_p99_iqr_t3[payload]=="" || p99_iqr[key]+0 < best_p99_iqr_t3[payload]+0) best_p99_iqr_t3[payload]=p99_iqr[key]
      }
      if (is_num(dropped[key])) {
        if (best_dropped[payload]=="" || dropped[key]+0 < best_dropped[payload]+0) best_dropped[payload]=dropped[key]
      }
    }
  }

  payload_blocks=0
  for (pi=1; pi<=2; pi++) {
    payload=payload_order[pi]
    if (!payload_present[payload]) continue
    payload_blocks++
    if (payload_blocks > 1) {
      print "| — | — | — | — | — | — | — |"
    }
    for (pj=1; pj<=4; pj++) {
      proxy=proxy_order[pj]
      key=proxy SUBSEP payload

      if (latency_seen[key]) {
        rpsm=fmt1(rps_med[key])
        rpsi=fmt3(rps_iqr[key])
        p99=fmt3(p99_ms[key])
        p99iq=fmt3(p99_iqr[key])
        drop=dropped[key]
        if (drop=="" || !is_num(drop)) drop="—"
        else drop=sprintf("%.0f", drop+0)

        if (rpsm != "—" && best_rps_med[payload] != "" && rps_med[key]+0 < best_rps_med[payload]+0) {
          rpsm=with_suffix(rpsm, "↓")
        }
        if (rpsi != "—" && best_rps_iqr[payload] != "" && rps_iqr[key]+0 > best_rps_iqr[payload]+0) {
          rpsi=with_suffix(rpsi, "↑")
        }
        if (p99 != "—" && best_p99_t3[payload] != "" && p99_ms[key]+0 > best_p99_t3[payload]+0) {
          p99=with_suffix(p99, "↑")
        }
        if (p99iq != "—" && best_p99_iqr_t3[payload] != "" && p99_iqr[key]+0 > best_p99_iqr_t3[payload]+0) {
          p99iq=with_suffix(p99iq, "↑")
        }
        if (drop != "—" && best_dropped[payload] != "" && dropped[key]+0 > best_dropped[payload]+0) {
          drop=with_suffix(drop, "↑")
        }
      } else {
        rpsm="—"
        rpsi="—"
        p99="—"
        p99iq="—"
        drop="—"
      }

      printf "| %-7s | %-7s | %7s | %7s | %6s | %7s | %7s |\n", proxy, payload, rpsm, rpsi, p99, p99iq, drop
    }
  }

  print ""
  print "---"
  print ""
  print "## Notes"
  print ""
  print "- CI results emphasize **trend stability**, not absolute peak numbers."
  print "- Payload split highlights control-path vs data-path sensitivity."
  print "- All tables are mechanically derived from summary.csv; no manual edits."
}
' "$SUMMARY_CSV" > "$REPORT_OUT"
