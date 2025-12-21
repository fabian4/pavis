#!/bin/bash

echo "🚀 Starting E2E Tests..."
echo "Waiting for Aegis to be ready..."
sleep 5

echo "---------------------------------------------------"
echo "📡 Sending 10 requests to http://localhost:8080/"
echo "---------------------------------------------------"

for i in {1..10}
do
   RESPONSE=$(curl -s http://localhost:8080/)
   UPSTREAM=$(echo $RESPONSE | grep -o '"service": *"[^"]*"' | cut -d'"' -f4)
   PROXY_HEADER=$(echo $RESPONSE | grep -o '"x-sidecar-proxy": *"[^"]*"' | cut -d'"' -f4)
   
   if [[ -z "$RESPONSE" ]]; then
     echo "❌ Request $i: FAILED (Empty Response)"
   else
     echo "✅ Request $i: Handled by $UPSTREAM | Proxy: $PROXY_HEADER"
   fi
   sleep 0.5
done

echo "---------------------------------------------------"
echo "Test Complete."
