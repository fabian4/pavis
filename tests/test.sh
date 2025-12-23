#!/bin/bash
set -e

URL="http://localhost:8080/"
MAX_RETRIES=30
count=0

echo "🚀 Starting E2E Tests..."
echo "Waiting for Pavis to be ready at $URL..."

# Wait for the service to return a 200 OK (or any success code)
until curl -s -f -o /dev/null "$URL" || [ $count -eq $MAX_RETRIES ]; do
  echo -n "."
  sleep 1
  count=$((count+1))
done
echo ""

if [ $count -eq $MAX_RETRIES ]; then
  echo "❌ Timeout waiting for Pavis to start."
  exit 1
fi

echo "✅ Pavis is up!"
echo "---------------------------------------------------"
echo "📡 Sending 10 requests..."
echo "---------------------------------------------------"

FAILURES=0
for i in {1..10}
do
   # Curl and capture output. Don't use -f here so we can see the error body if any.
   RESPONSE=$(curl -s "$URL")
   
   # Check for expected upstream content
   if echo "$RESPONSE" | grep -q "backend-v"; then
     # Try to extract the service name for display (assuming echo-server format)
     # We look for "SERVICE_NAME":"backend-vX"
     UPSTREAM=$(echo "$RESPONSE" | grep -o '"SERVICE_NAME":"[^"]*"' | cut -d'"' -f4)
     
     # Fallback if specific grep fails but we know it's a backend
     if [ -z "$UPSTREAM" ]; then
         UPSTREAM="backend-v(1/2)"
     fi
     
     echo "✅ Request $i: Handled by $UPSTREAM"
   else
     echo "❌ Request $i: FAILED. Unexpected Response."
     # echo "Response: $RESPONSE" # Uncomment for debugging
     FAILURES=$((FAILURES+1))
   fi
   sleep 0.2
done

echo "---------------------------------------------------"
if [ $FAILURES -eq 0 ]; then
  echo "🎉 All tests passed!"
  exit 0
else
  echo "💥 $FAILURES requests failed."
  exit 1
fi