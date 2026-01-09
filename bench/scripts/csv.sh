#!/bin/bash
# Generate CSV report from benchmark results
# Enhanced with multi-run statistical aggregation and new metrics
#
# Data Sources:
# =============
# | Column              | Source                                   |
# |---------------------|------------------------------------------|
# | proxy               | filename + "# Version:" header           |
# | run_id              | config string from "Config:" line        |
# | workload            | config: ..._{workload}_...               |
# | resource_profile    | config: ..._{resource}_...               |
# | duration_s          | config: short=30, extended=300           |
# | load_type           | "Load Type:" line in output              |
# | backend_type        | "Backend:" line in output                |
# | connections         | wrk output: "X threads and Y conn"       |
# | threads             | wrk output: "X threads and Y conn"       |
# | target_rps          | wrk2: target rate (if open-loop)         |
# | achieved_rps        | wrk output: "Requests/sec:"              |
# | rps_median          | median of multi-run RPS values           |
# | rps_iqr             | IQR of multi-run RPS values              |
# | keepalive           | workload: churn=false, else=true         |
# | avg_latency_ms      | wrk output: "Latency" Avg column         |
# | p50_ms              | wrk output: "50%" line                   |
# | p90_ms              | wrk output: "90%" line                   |
# | p99_ms              | wrk output: "99%" line                   |
# | p999_ms             | wrk output: "99.9%" line (wrk2)          |
# | p99_median          | median of multi-run p99 values           |
# | p99_iqr             | IQR of multi-run p99 values              |
# | errors              | wrk output: "Socket errors" sum          |
# | peak_cpu_pct        | txt: "Resource Stats:" (max CPU %)       |
# | avg_cpu_pct         | txt: "Resource Stats:" (avg CPU %)       |
# | peak_mem_mib        | txt: "Resource Stats:" (max Mem MiB)     |
# | avg_mem_mib         | txt: "Resource Stats:" (avg Mem MiB)     |
# | backend_cpu_pct     | Backend CPU usage (avg)                  |
# | backend_saturated   | Flag if backend CPU > 80%                |
# | run_count           | Number of iterations for this config     |

RESULTS_DIR="${RESULTS_DIR:-bench/output}"
OUTPUT_DIR="${RESULTS_DIR}"

# Extract metadata from first available txt file
extract_metadata() {
    for proxy in pavis envoy nginx haproxy; do
        local txt_file="${RESULTS_DIR}/${proxy}/${proxy}.txt"
        if [ -f "$txt_file" ]; then
            TIMESTAMP=$(grep "^# Generated:" "$txt_file" | cut -d' ' -f3)
            return
        fi
    done
    # Fallback
    TIMESTAMP=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
}

extract_metadata

# Calculate median from a list of values
calc_median() {
    local values="$1"
    echo "$values" | tr ' ' '\n' | sort -n | awk '{
        a[NR] = $1
    }
    END {
        if (NR == 0) print 0
        else if (NR % 2 == 1) print a[(NR+1)/2]
        else print (a[NR/2] + a[NR/2+1]) / 2
    }'
}

# Calculate IQR (interquartile range) from a list of values
calc_iqr() {
    local values="$1"
    echo "$values" | tr ' ' '\n' | sort -n | awk '{
        a[NR] = $1
    }
    END {
        if (NR < 2) { print 0; exit }
        q1_idx = int(NR * 0.25)
        q3_idx = int(NR * 0.75)
        if (q1_idx < 1) q1_idx = 1
        if (q3_idx > NR) q3_idx = NR
        printf "%.2f", a[q3_idx] - a[q1_idx]
    }'
}

# Parse resource stats from content block
# Returns: peak_cpu_pct,avg_cpu_pct,peak_mem_mib,avg_mem_mib
get_resource_stats() {
    local content="$1"
    local stats_type="${2:-Proxy}"  # Proxy or Backend

    # Extract lines after "Resource Stats:" and before next section (separator line)
    # Format from run.sh: "{{.CPUPerc}},{{.MemUsage}}" -> "19.18%,96.66MiB / 512MiB"
    echo "$content" | sed -n "/${stats_type} Resource Stats:/,/^-/p" | grep -v "Resource Stats:" | grep -v "^-" | grep -E '^[0-9]' | awk -F',' '{
        # $1 is "19.18%"
        # $2 is "96.66MiB / 512MiB"

        # CPU
        cpu=$1; gsub(/%/, "", cpu)

        # Memory: take the first part of "96.66MiB / 512MiB"
        split($2, mem_parts, " / ")
        mem=mem_parts[1]

        if (mem ~ /GiB/) { gsub(/GiB/, "", mem); mem=mem*1024 }
        else if (mem ~ /MiB/) { gsub(/MiB/, "", mem) }
        else if (mem ~ /KiB/) { gsub(/KiB/, "", mem); mem=mem/1024 }
        else if (mem ~ /B/) { gsub(/B/, "", mem); mem=mem/1024/1024 }

        cpu_sum += cpu; if (cpu > cpu_max) cpu_max = cpu
        mem_sum += mem; if (mem > mem_max) mem_max = mem
        count++
    }
    END {
        if (count > 0) {
            printf "%.2f,%.2f,%.2f,%.2f", cpu_max, cpu_sum/count, mem_max, mem_sum/count
        } else {
            print "0,0,0,0"
        }
    }'
}

# Process multi-iteration wrk blocks and aggregate
process_wrk_block() {
    local content=$1
    local proxy=$2
    local version=$3
    local config=$4
    
    # Append version to proxy name if available
    local proxy_display="$proxy"
    if [ -n "$version" ] && [ "$version" != "unknown" ]; then
        proxy_display="${proxy}(${version})"
    fi

    # Extract metadata from content header
    local load_type=$(echo "$content" | grep "^Load Type:" | cut -d':' -f2 | xargs)
    local backend_type=$(echo "$content" | grep "^Backend:" | cut -d':' -f2 | xargs)
    local run_count=$(echo "$content" | grep "^Runs:" | cut -d':' -f2 | xargs)

    # Parse config: workload_resource_duration_intensity
    local workload=$(echo "$config" | cut -d'_' -f1)
    local resource=$(echo "$config" | cut -d'_' -f2)
    local duration_type=$(echo "$config" | cut -d'_' -f3)

    # Defaults
    : "${load_type:=closed-loop}"
    : "${backend_type:=httpbin}"
    : "${run_count:=1}"
    
    # Extract metrics from wrk output (aggregate across iterations if multi-run)
    local conn_thr=$(echo "$content" | grep "threads and" | head -1 | awk '{print $1","$4}')
    local threads=$(echo "$conn_thr" | cut -d',' -f1)
    local connections=$(echo "$conn_thr" | cut -d',' -f2)

    # Extract target RPS for open-loop workloads (wrk2 specific)
    local target_rps=0
    if [ "$load_type" = "open-loop" ]; then
        # wrk2 doesn't explicitly show target in output, but we know from config
        # This is a limitation - we'd need to parse from command line or config
        target_rps="N/A"
    fi

    # Collect RPS and P99 from all iterations for statistical aggregation
    local rps_values=$(echo "$content" | grep "Requests/sec:" | awk '{print $2}' | tr '\n' ' ')
    local rps=$(echo "$rps_values" | awk '{print $1}')  # First iteration value
    local rps_median=$(calc_median "$rps_values")
    local rps_iqr=$(calc_iqr "$rps_values")
    
    local lat_line=$(echo "$content" | grep "Latency" | head -1)
    local avg_lat=$(echo "$lat_line" | awk '{v=$2; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}')
    local stdev_lat=$(echo "$lat_line" | awk '{v=$3; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}')
    local max_lat=$(echo "$lat_line" | awk '{v=$4; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}')
    
    local p50=$(echo "$content" | awk '$1=="50%" {v=$2; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}' | head -1)
    local p75=$(echo "$content" | awk '$1=="75%" {v=$2; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}' | head -1)
    local p90=$(echo "$content" | awk '$1=="90%" {v=$2; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}' | head -1)

    # Collect P99 from all iterations
    local p99_values=$(echo "$content" | awk '$1=="99%" {v=$2; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}' | tr '\n' ' ')
    local p99=$(echo "$p99_values" | awk '{print $1}')  # First iteration
    local p99_median=$(calc_median "$p99_values")
    local p99_iqr=$(calc_iqr "$p99_values")

    # P99.9 (wrk2 specific)
    local p999=$(echo "$content" | awk '$1=="99.9%" || $1=="99.90%" {v=$2; gsub(/us/,"",v); if(v~/ms/){gsub(/ms/,"",v)} else if(v~/s$/){gsub(/s/,"",v);v=v*1000} else {v=v/1000}; print v}' | head -1)
    
    local total_req=$(echo "$content" | grep "requests in" | awk '{print $1}')
    local total_bytes=$(echo "$content" | grep "requests in" | awk '{
        val=$(NF-1)
        if(val~/MB/){gsub(/MB/,"",val);val=val*1024*1024}
        else if(val~/KB/){gsub(/KB/,"",val);val=val*1024}
        else if(val~/GB/){gsub(/GB/,"",val);val=val*1024*1024*1024}
        printf "%.0f",val
    }')
    
    local transfer=$(echo "$content" | grep "Transfer/sec:" | awk '{v=$2; gsub(/KB/,"",v); if(v~/MB/){gsub(/MB/,"",v);v=v*1024}; print v}')
    
    local rps_line=$(echo "$content" | grep "Req/Sec")
    local avg_rps_thr=$(echo "$rps_line" | awk '{v=$2; if(v~/k$/){gsub(/k/,"",v);v=v*1000}; print v}')
    local stdev_rps_thr=$(echo "$rps_line" | awk '{v=$3; if(v~/k$/){gsub(/k/,"",v);v=v*1000}; print v}')
    
    local errors=$(echo "$content" | grep -E "Socket errors:" | awk '{sum=0; for(i=1;i<=NF;i++) if($i~/^[0-9]+$/) sum+=$i; print sum}')
    
    # Derived values
    local duration=$([ "$duration_type" = "extended" ] && echo "300" || echo "30")
    local keepalive=$([ "$workload" = "churn" ] && echo "false" || echo "true")
    
    # Resource stats (Proxy)
    local res_stats=$(get_resource_stats "$content" "Proxy")
    local peak_cpu=$(echo "$res_stats" | cut -d',' -f1)
    local avg_cpu=$(echo "$res_stats" | cut -d',' -f2)
    local peak_mem=$(echo "$res_stats" | cut -d',' -f3)
    local avg_mem=$(echo "$res_stats" | cut -d',' -f4)

    # Backend stats
    local backend_stats=$(get_resource_stats "$content" "Backend")
    local backend_cpu=$(echo "$backend_stats" | cut -d',' -f2)  # avg CPU
    local backend_saturated="false"
    if awk -v cpu="$backend_cpu" 'BEGIN { exit !(cpu > 80) }' 2>/dev/null; then
        backend_saturated="true"
    fi
    
    # Defaults
    : "${connections:=0}"
    : "${threads:=4}"
    : "${rps:=0}"
    : "${avg_lat:=0}"
    : "${stdev_lat:=0}"
    : "${max_lat:=0}"
    : "${p50:=0}"
    : "${p75:=0}"
    : "${p90:=0}"
    : "${p99:=0}"
    : "${p999:=0}"
    : "${p99_median:=0}"
    : "${p99_iqr:=0}"
    : "${rps_median:=0}"
    : "${rps_iqr:=0}"
    : "${total_req:=0}"
    : "${total_bytes:=0}"
    : "${transfer:=0}"
    : "${avg_rps_thr:=0}"
    : "${stdev_rps_thr:=0}"
    : "${errors:=0}"
    : "${backend_cpu:=0}"

    # Output CSV row with new metrics
    # Order: proxy, run_id, workload, resource, duration, load_type, backend_type, connections, threads, target_rps, achieved_rps, rps_median, rps_iqr, keepalive, avg_latency_ms, p50_ms, p90_ms, p99_ms, p999_ms, p99_median, p99_iqr, errors, peak_cpu_pct, avg_cpu_pct, peak_mem_mib, avg_mem_mib, backend_cpu_pct, backend_saturated, run_count
    echo "${proxy_display},${config},${workload},${resource},${duration},${load_type},${backend_type},${connections},${threads},${target_rps},${rps},${rps_median},${rps_iqr},${keepalive},${avg_lat},${p50},${p90},${p99},${p999},${p99_median},${p99_iqr},${errors},${peak_cpu},${avg_cpu},${peak_mem},${avg_mem},${backend_cpu},${backend_saturated},${run_count}" >> "$OUTPUT_FILE"
}

# Process consolidated txt file (pavis.txt, envoy.txt, etc.)
process_consolidated_txt() {
    local txt_file=$1
    local proxy=$2
    
    local current_config=""
    local current_block=""
    local in_block=false
    local version=""
    
    # Get version from header if available
    version=$(grep "^# Version:" "$txt_file" | cut -d' ' -f3)
    
    while IFS= read -r line || [ -n "$line" ]; do
        if [[ "$line" =~ ^Config:\ (.+)$ ]]; then
            # Save previous block if exists
            if [ -n "$current_config" ] && [ -n "$current_block" ]; then
                process_wrk_block "$current_block" "$proxy" "$version" "$current_config"
            fi
            current_config="${BASH_REMATCH[1]}"
            current_block=""
            in_block=true
        elif [[ "$line" =~ ^===+ ]]; then
            continue
        elif [ "$in_block" = true ]; then
            current_block+="$line"$'\n'
        fi
    done < "$txt_file"
    
    # Process last block
    if [ -n "$current_config" ] && [ -n "$current_block" ]; then
        process_wrk_block "$current_block" "$proxy" "$version" "$current_config"
    fi
}

# CSV header (updated with new metrics)
CSV_HEADER="proxy,run_id,workload,resource_profile,duration_s,load_type,backend_type,connections,threads,target_rps,achieved_rps,rps_median,rps_iqr,keepalive,avg_latency_ms,p50_ms,p90_ms,p99_ms,p999_ms,p99_median,p99_iqr,errors,peak_cpu_pct,avg_cpu_pct,peak_mem_mib,avg_mem_mib,backend_cpu_pct,backend_saturated,run_count"

# Single output file
OUTPUT_FILE="${OUTPUT_DIR}/results.csv"
echo "$CSV_HEADER" > "$OUTPUT_FILE"

# Process consolidated txt files (e.g., pavis.txt, envoy.txt)
for proxy in pavis envoy nginx haproxy; do
    txt_file="${RESULTS_DIR}/${proxy}/${proxy}.txt"
    
    if [ -f "$txt_file" ]; then
        # Process consolidated txt file
        process_consolidated_txt "$txt_file" "$proxy"
    fi
done

# Sort by proxy, workload, resource
head -1 "$OUTPUT_FILE" > "${OUTPUT_FILE}.tmp"
tail -n +2 "$OUTPUT_FILE" | sort -t',' -k1,1 -k3,3 -k4,4 >> "${OUTPUT_FILE}.tmp"
mv "${OUTPUT_FILE}.tmp" "$OUTPUT_FILE"

echo "Generated: $OUTPUT_FILE ($(tail -n +2 "$OUTPUT_FILE" | wc -l | tr -d ' ') rows)"
echo ""
echo "Enhanced metrics included:"
echo "  - Load type (open-loop vs closed-loop)"
echo "  - Backend type (httpbin vs minimal)"
echo "  - Multi-run statistics (median, IQR)"
echo "  - P99.9 latency (wrk2)"
echo "  - Backend saturation indicators"
