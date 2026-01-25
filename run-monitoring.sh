#!/bin/bash
# Run monitoring for 70 minutes with 10-minute intervals (7 checks total)

echo "$(date): Starting 70-minute monitoring (7 checks at 10-minute intervals)"

for i in {1..7}; do
    echo ""
    echo "========================================="
    echo "$(date): CHECK $i/7"
    echo "========================================="

    /tmp/monitor-fetch.sh
    EXIT_CODE=$?

    # If stalled (exit code 3), stop monitoring and report
    if [ $EXIT_CODE -eq 3 ]; then
        echo ""
        echo "========================================="
        echo "$(date): STALL DETECTED - Stopping monitoring"
        echo "$(date): Awaiting code fix and manual restart"
        echo "========================================="
        exit 3
    fi

    # If crashed (exit code 2), stop monitoring and report
    if [ $EXIT_CODE -eq 2 ]; then
        echo ""
        echo "========================================="
        echo "$(date): PROCESS CRASHED - Stopping monitoring"
        echo "$(date): Awaiting investigation and manual restart"
        echo "========================================="
        exit 2
    fi

    if [ $i -lt 7 ]; then
        echo "$(date): Next check in 10 minutes..."
        sleep 600  # 10 minutes
    fi
done

echo ""
echo "========================================="
echo "$(date): 70-minute monitoring complete"
echo "========================================="
