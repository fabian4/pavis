#!/bin/bash
# summary.sh - Generate benchmark analysis report from results.csv
# Output: 7 sections covering overview, deltas, performance, stability, resources, errors, summary

set -e

RESULTS_DIR="${RESULTS_DIR:-bench/output}"
CSV_FILE="${RESULTS_DIR}/results.csv"
SUMMARY_FILE="${RESULTS_DIR}/summary.md"

if [ ! -f "$CSV_FILE" ]; then
    echo "Error: $CSV_FILE not found. Run csv.sh first."
    exit 1
fi

# Extract metadata from first available txt file
extract_metadata() {
    for proxy in pavis envoy nginx haproxy; do
        local txt_file="${RESULTS_DIR}/${proxy}/${proxy}.txt"
        if [ -f "$txt_file" ]; then
            TIMESTAMP=$(grep "^# Generated:" "$txt_file" | cut -d' ' -f3)
            return
        fi
    done
    # Fallback: use current time
    TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
}

extract_metadata
TOTAL_ROWS=$(tail -n +2 "$CSV_FILE" | wc -l | tr -d ' ')
PROXIES=($(tail -n +2 "$CSV_FILE" | cut -d',' -f2 | sort -u))
WORKLOADS=($(tail -n +2 "$CSV_FILE" | cut -d',' -f3 | sort -u))
RESOURCES=($(tail -n +2 "$CSV_FILE" | cut -d',' -f4 | sort -u))

# Determine baseline duration (most frequent duration in CSV)
BASELINE_DURATION=$(tail -n +2 "$CSV_FILE" | cut -d',' -f5 | sort | uniq -c | sort -nr | head -n1 | awk '{print $2}')
[ -z "$BASELINE_DURATION" ] && BASELINE_DURATION="30"

get_version() {
    local proxy=$1
    local txt_file="${RESULTS_DIR}/${proxy}/${proxy}.txt"
    if [ -f "$txt_file" ]; then
        grep "^# Version:" "$txt_file" | cut -d' ' -f3
    else
        echo "unknown"
    fi
}

# CSV column indices (1-based)
# 1:proxy, 2:run_id, 3:workload, 4:resource_profile, 5:duration_s
# 6:connections, 7:threads, 8:keepalive, 9:rps, 10:avg_latency_ms, 11:stdev_latency_ms
# 12:max_latency_ms, 13:p50_ms, 14:p75_ms, 15:p90_ms, 16:p99_ms, 17:total_requests
# 18:total_bytes, 19:transfer_kb_s, 20:avg_rps_thread, 21:stdev_rps_thread, 22:errors
# 23:peak_cpu_pct, 24:avg_cpu_pct, 25:peak_mem_mib, 26:avg_mem_mib

TOTAL_ROWS=$(tail -n +2 "$CSV_FILE" | wc -l | tr -d ' ')
PROXIES=($(tail -n +2 "$CSV_FILE" | cut -d',' -f1 | sort -u))
WORKLOADS=($(tail -n +2 "$CSV_FILE" | cut -d',' -f3 | sort -u))
RESOURCES=($(tail -n +2 "$CSV_FILE" | cut -d',' -f4 | sort -u))

get_row() {
    # $1=proxy, $2=workload, $3=resource, $4=duration, $5=connections
    tail -n +2 "$CSV_FILE" | awk -F',' -v p="$1" -v w="$2" -v r="$3" -v d="$4" -v c="$5" \
        '$1==p && $3==w && $4==r && $5==d && $6==c {print; exit}'
}

get_field() {
    echo "$1" | cut -d',' -f"$2"
}

get_conn() {
    case $1 in
        throughput) echo 100 ;;
        latency) echo 500 ;;
        concurrency) echo 5000 ;;
        churn) echo 100 ;;
        *) echo 100 ;;
    esac
}

calc_delta() {
    local val=$1 base=$2
    [ -z "$base" ] || [ -z "$val" ] && return
    # Check if base is 0 (float aware)
    if awk -v b="$base" 'BEGIN { exit !(b == 0) }'; then
        return
    fi
    awk -v v="$val" -v b="$base" 'BEGIN { printf "%.1f", (v - b) / b * 100 }'
}

format_delta() {
    local val=$1
    [ -z "$val" ] && echo "N/A" && return
    awk -v v="$val" 'BEGIN {
        if (v == 0) printf "0.0%%"
        else if (v > 0) printf "+%.1f%%", v
        else printf "%.1f%%", v
    }'
}

# Capture Git metadata
GIT_TAG=${PAVIS_TAG:-bench}
GIT_COMMIT=${PAVIS_COMMIT:-unknown}

generate_report() {
    # ===================
    # Section 1: Overview (compact format)
    # ===================
    local proxies_fmt=""
    local workloads_fmt=""
    local resources_fmt=""
    
    for p in "${PROXIES[@]}"; do
        [ -n "$proxies_fmt" ] && proxies_fmt+=" · "
        proxies_fmt+="\`$p\`"
    done
    for w in "${WORKLOADS[@]}"; do
        [ -n "$workloads_fmt" ] && workloads_fmt+=" · "
        workloads_fmt+="\`$w\`"
    done
    for r in "${RESOURCES[@]}"; do
        [ -n "$resources_fmt" ] && resources_fmt+=" · "
        resources_fmt+="\`$r\`"
    done
    
    cat <<EOF
# Benchmark Analysis Report

## 1. Overview

**Generated:** \`$TIMESTAMP\` ｜ **Runs:** \`$TOTAL_ROWS\` · **Baseline:** \`envoy\`

**Version:** \`$GIT_TAG\` (\`$GIT_COMMIT\`)

> Proxies: $proxies_fmt  
> Workloads: $workloads_fmt  
> Profiles: $resources_fmt

---

EOF

    # ===================
    # Section 2: Baseline Consolidated Table
    # ===================
    echo "## 2. Baseline Consolidated Table"
    echo ""
    echo "Filters: resource_profile = baseline, duration_s = $BASELINE_DURATION"
    echo ""
    echo "| Proxy | Throughput RPS | Latency RPS | Concurrency RPS | Churn RPS | Avg CPU (%) | Avg Memory (MiB) |"
    echo "|-------|----------------|-------------|-----------------|-----------|-------------|------------------|"

    for proxy in "${PROXIES[@]}"; do
        local rps_t="" rps_l="" rps_c="" rps_ch=""
        local cpu_sum=0 mem_sum=0 count=0
        
        for workload in throughput latency concurrency churn; do
            local conn=$(get_conn "$workload")
            local row=$(get_row "$proxy" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
            [ -z "$row" ] && continue
            local rps=$(get_field "$row" 9)
            local cpu=$(get_field "$row" 24)
            local mem=$(get_field "$row" 26)
            
            case $workload in
                throughput) rps_t="$rps" ;;
                latency) rps_l="$rps" ;;
                concurrency) rps_c="$rps" ;;
                churn) rps_ch="$rps" ;;
            esac
            
            if [ -n "$cpu" ] && [ -n "$mem" ]; then
                cpu_sum=$(awk -v s="$cpu_sum" -v c="$cpu" 'BEGIN { print s + c }')
                mem_sum=$(awk -v s="$mem_sum" -v m="$mem" 'BEGIN { print s + m }')
                count=$((count + 1))
            fi
        done
        
        local avg_cpu=$(awk -v s="$cpu_sum" -v c="$count" 'BEGIN { printf "%.1f", (c > 0) ? s/c : 0 }')
        local avg_mem=$(awk -v s="$mem_sum" -v c="$count" 'BEGIN { printf "%.0f", (c > 0) ? s/c : 0 }')
        
        [ -z "$rps_ch" ] && rps_ch="N/A"
        
        local proxy_name=$(echo "$proxy" | sed 's/([^)]*)//g')
        echo "| $proxy_name | $rps_t | $rps_l | $rps_c | $rps_ch | $avg_cpu | $avg_mem |"
    done
    echo ""
    echo "---"
    echo ""

    # ===================
    # Section 3: Workload Performance Tables
    # ===================
    echo "## 3. Workload Performance Tables"
    echo ""

    for workload in throughput latency concurrency churn; do
        local conn=$(get_conn "$workload")
        local envoy_row=$(get_row "envoy" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
        # Try to find envoy row with version if generic name fails
        if [ -z "$envoy_row" ]; then
             # Find the specific envoy name used in this run (e.g., envoy(v1.32))
             local envoy_name=$(echo "${PROXIES[@]}" | tr ' ' '\n' | grep "^envoy" | head -n1)
             [ -n "$envoy_name" ] && envoy_row=$(get_row "$envoy_name" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
        fi
        local envoy_rps=$(get_field "$envoy_row" 9)
        
        echo "### ${workload^} (baseline, ${BASELINE_DURATION}s, $conn connections)"
        echo ""
        echo "| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |"
        echo "|-------|------------------|------------------|--------|-------------------|-------------------|"
        
        for proxy in "${PROXIES[@]}"; do
            local row=$(get_row "$proxy" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
            [ -z "$row" ] && continue
            local rps=$(get_field "$row" 9)
            local p99=$(get_field "$row" 16)
            local errors=$(get_field "$row" 22)
            local avg_cpu=$(get_field "$row" 24)
            local avg_mem=$(get_field "$row" 26)
            local rps_cpu=$(awk -v r="$rps" -v c="$avg_cpu" 'BEGIN { printf "%.2f", (c > 0) ? r/c : 0 }')
            local rps_mem=$(awk -v r="$rps" -v m="$avg_mem" 'BEGIN { printf "%.2f", (m > 0) ? r/m : 0 }')
            local rps_display
            # Check if proxy starts with "envoy"
            if [[ "$proxy" == envoy* ]]; then
                rps_display="$rps"
            else
                local delta=$(format_delta "$(calc_delta "$rps" "$envoy_rps")")
                rps_display="$rps ($delta)"
            fi
            local proxy_name=$(echo "$proxy" | sed 's/([^)]*)//g')
            echo "| $proxy_name | $rps_display | $p99 | $errors | $avg_cpu ($rps_cpu) | $avg_mem ($rps_mem) |"
        done
        echo ""

        # Insert 2x Intensity Table for Concurrency
        if [ "$workload" = "concurrency" ]; then
            echo "### Concurrency (2x intensity, 30s, 10000 connections)"
            echo ""
            echo "| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |"
            echo "|-------|------------------|------------------|--------|-------------------|-------------------|"
            
            local conn_2x="10000"
            local workload_2x="concurrency"
            
            # Get Envoy baseline for 2x
            local envoy_row_2x=$(get_row "envoy" "$workload_2x" "baseline" "$BASELINE_DURATION" "$conn_2x")
            if [ -z "$envoy_row_2x" ]; then
                 local envoy_name=$(echo "${PROXIES[@]}" | tr ' ' '\n' | grep "^envoy" | head -n1)
                 [ -n "$envoy_name" ] && envoy_row_2x=$(get_row "$envoy_name" "$workload_2x" "baseline" "$BASELINE_DURATION" "$conn_2x")
            fi
            local envoy_rps_2x=$(get_field "$envoy_row_2x" 9)

            for proxy in "${PROXIES[@]}"; do
                local row=$(get_row "$proxy" "$workload_2x" "baseline" "$BASELINE_DURATION" "$conn_2x")
                [ -z "$row" ] && continue
                local rps=$(get_field "$row" 9)
                local p99=$(get_field "$row" 16)
                local errors=$(get_field "$row" 22)
                local avg_cpu=$(get_field "$row" 24)
                local avg_mem=$(get_field "$row" 26)
                local rps_cpu=$(awk -v r="$rps" -v c="$avg_cpu" 'BEGIN { printf "%.2f", (c > 0) ? r/c : 0 }')
                local rps_mem=$(awk -v r="$rps" -v m="$avg_mem" 'BEGIN { printf "%.2f", (m > 0) ? r/m : 0 }')
                local rps_display
                
                if [[ "$proxy" == envoy* ]]; then
                    rps_display="$rps"
                else
                    local delta=$(format_delta "$(calc_delta "$rps" "$envoy_rps_2x")")
                    rps_display="$rps ($delta)"
                fi
                local proxy_name=$(echo "$proxy" | sed 's/([^)]*)//g')
                echo "| $proxy_name | $rps_display | $p99 | $errors | $avg_cpu ($rps_cpu) | $avg_mem ($rps_mem) |"
            done
            echo ""
        fi
    done

    echo "---"
    echo ""

    # ===================
    # Section 4: Stability (30s vs 300s)
    # ===================
    echo "## 4. Stability (${BASELINE_DURATION}s vs 300s)"
    echo ""
    echo "| Proxy | Workload | RPS (${BASELINE_DURATION}s) | RPS (300s) | Delta (%) |"
    echo "|-------|----------|-----------|------------|-----------|"

    for proxy in "${PROXIES[@]}"; do
        for workload in throughput latency; do
            local conn=$(get_conn "$workload")
            local row_30=$(get_row "$proxy" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
            local row_300=$(get_row "$proxy" "$workload" "baseline" "300" "$conn")
            [ -z "$row_30" ] || [ -z "$row_300" ] && continue
            local rps_30=$(get_field "$row_30" 9)
            local rps_300=$(get_field "$row_300" 9)
            local delta=$(format_delta "$(calc_delta "$rps_300" "$rps_30")")
            local proxy_name=$(echo "$proxy" | sed 's/([^)]*)//g')
            echo "| $proxy_name | $workload | $rps_30 | $rps_300 | $delta |"
        done
    done
    echo ""
    echo "---"
    echo ""

    # ===================
    # Section 5: Resource Efficiency
    # ===================
    echo "## 5. Resource Efficiency"
    echo ""
    echo "Filter: throughput workload, resource_profile=baseline, duration=${BASELINE_DURATION}s"
    echo ""
    echo "| Proxy | Avg CPU (%) | Peak CPU (%) | Avg Mem (MiB) | Peak Mem (MiB) | RPS/CPU | RPS/MiB |"
    echo "|-------|-------------|--------------|---------------|----------------|---------|---------|"

    for proxy in "${PROXIES[@]}"; do
        local row=$(get_row "$proxy" "throughput" "baseline" "$BASELINE_DURATION" "100")
        [ -z "$row" ] && continue
        local rps=$(get_field "$row" 9)
        local avg_cpu=$(get_field "$row" 24)
        local peak_cpu=$(get_field "$row" 23)
        local avg_mem=$(get_field "$row" 26)
        local peak_mem=$(get_field "$row" 25)
        local rps_cpu=$(awk -v r="$rps" -v c="$avg_cpu" 'BEGIN { printf "%.2f", (c > 0) ? r/c : 0 }')
        local rps_mem=$(awk -v r="$rps" -v m="$avg_mem" 'BEGIN { printf "%.2f", (m > 0) ? r/m : 0 }')
        local proxy_name=$(echo "$proxy" | sed 's/([^)]*)//g')
        echo "| $proxy_name | $avg_cpu | $peak_cpu | $avg_mem | $peak_mem | $rps_cpu | $rps_mem |"
    done
    echo ""
    echo "---"
    echo ""

    # ===================
    # Section 6: Error Overview
    # ===================
    echo "## 6. Error Overview"
    echo ""
    
    # Dynamic header based on available proxies
    local header="| Workload | Connections |"
    local separator="|----------|-------------|"
    for proxy in "${PROXIES[@]}"; do
        local proxy_name=$(echo "$proxy" | sed 's/([^)]*)//g')
        header+=" $proxy_name |"
        separator+="-------|"
    done
    echo "$header"
    echo "$separator"

    for workload in throughput latency concurrency churn; do
        local conn=$(get_conn "$workload")
        local row_out="| $workload | $conn |"
        for proxy in "${PROXIES[@]}"; do
            local row=$(get_row "$proxy" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
            local err=$(get_field "$row" 22)
            [ -z "$err" ] && err="N/A"
            row_out+=" $err |"
        done
        echo "$row_out"

        # Add extra rows for 2x intensity workloads
        if [ "$workload" = "latency" ] || [ "$workload" = "concurrency" ]; then
             local conn_2x=$([ "$workload" = "latency" ] && echo "1000" || echo "10000")
             local row_2x_out="| $workload (2x) | $conn_2x |"
             for proxy in "${PROXIES[@]}"; do
                 local row_2x=$(get_row "$proxy" "$workload" "baseline" "$BASELINE_DURATION" "$conn_2x")
                 local err_2x=$(get_field "$row_2x" 22)
                 [ -z "$err_2x" ] && err_2x="N/A"
                 row_2x_out+=" $err_2x |"
             done
             echo "$row_2x_out"
        fi
    done
    echo ""
    echo "---"
    echo ""

    # ===================
    # Section 7: Key Findings
    # ===================
    echo "## 7. Key Findings"
    echo ""

    # Build RPS rankings for each workload
    for workload in throughput latency concurrency churn; do
        local conn=$(get_conn "$workload")
        local rps_list=""
        
        for proxy in "${PROXIES[@]}"; do
            local row=$(get_row "$proxy" "$workload" "baseline" "$BASELINE_DURATION" "$conn")
            local rps=$(get_field "$row" 9)
            [ -n "$rps" ] && [ "$rps" != "0" ] && rps_list+="$rps $proxy\n"
        done
        
        # Sort descending and format as ranking
        local sorted=$(echo -e "$rps_list" | sort -rn | awk 'NF{print $2}' | paste -sd ' ' -)
        local ranking=$(echo "$sorted" | sed 's/([^)]*)//g' | sed 's/ / > /g')
        
        [ -n "$ranking" ] && echo "**${workload^}:** $ranking" && echo ""
    done
    
    # Resource efficiency (lowest usage)
    local min_cpu_proxy="" min_cpu=999999
    local min_mem_proxy="" min_mem=999999
    
    for proxy in "${PROXIES[@]}"; do
        local row=$(get_row "$proxy" "throughput" "baseline" "$BASELINE_DURATION" "100")
        [ -z "$row" ] && continue
        local avg_cpu=$(get_field "$row" 24)
        local avg_mem=$(get_field "$row" 26)
        
        if awk -v c="$avg_cpu" -v m="$min_cpu" 'BEGIN { exit !(c < m) }'; then
            min_cpu="$avg_cpu"
            min_cpu_proxy=$(echo "$proxy" | sed 's/([^)]*)//g')
        fi
        if awk -v m="$avg_mem" -v n="$min_mem" 'BEGIN { exit !(m < n) }'; then
            min_mem="$avg_mem"
            min_mem_proxy=$(echo "$proxy" | sed 's/([^)]*)//g')
        fi
    done
    
    echo "**Lowest CPU:** $min_cpu_proxy ($min_cpu%)"
    echo ""
    echo "**Lowest Memory:** $min_mem_proxy (${min_mem} MiB)"
    
    # Errors summary
    local workloads_with_errors=""
    for workload in throughput latency concurrency churn; do
        local conn=$(get_conn "$workload")
        for proxy in "${PROXIES[@]}"; do
            local row=$(get_row "$proxy" "$workload" "baseline" "30" "$conn")
            local errors=$(get_field "$row" 22)
            if [ -n "$errors" ] && [ "$errors" != "0" ]; then
                if [[ ! "$workloads_with_errors" =~ "$workload" ]]; then
                    [ -n "$workloads_with_errors" ] && workloads_with_errors+=", "
                    workloads_with_errors+="$workload"
                fi
                break
            fi
        done
    done
    
    echo ""
    if [ -n "$workloads_with_errors" ]; then
        echo "**Errors observed:** $workloads_with_errors"
    else
        echo "**Errors observed:** none"
    fi

    echo ""
    echo "---"
    echo ""
    echo "All results are derived directly from results.csv."
}

generate_report > "$SUMMARY_FILE"
echo "Summary written to: $SUMMARY_FILE"
