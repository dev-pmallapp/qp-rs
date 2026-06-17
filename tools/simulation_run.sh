#!/usr/bin/env bash

# Redirection of env directories for renode
export XDG_CONFIG_HOME=/home/pmallapp/Devel/Embedded/qp-rs.git/.renode_home/config
export XDG_DATA_HOME=/home/pmallapp/Devel/Embedded/qp-rs.git/.renode_home/share
export XDG_CACHE_HOME=/home/pmallapp/Devel/Embedded/qp-rs.git/.renode_home/cache

echo "Launching Renode in the background..."
renode --disable-xwt -P 1234 configs/renode/scripts/esp32c6_dpp.resc > renode_stdout.log 2>&1 &
RENODE_PID=$!

# Wait for Renode to initialize and start the TCP socket server
echo "Waiting for Renode to start TCP socket server on port 7777..."
sleep 15

# Launch qspy in the background, redirecting output to workspace file
echo "Connecting qspy..."
./target/debug/qspy --tcp-remote 127.0.0.1:7777 --no-kbd > /home/pmallapp/Devel/Embedded/qp-rs.git/qspy_telemetry.log 2>&1 &
QSPY_PID=$!

# Let the simulation run for 25 seconds to capture dining philosophers transitions
echo "Simulation running (capturing traces for 25 seconds)..."
sleep 25

# Clean up
echo "Terminating simulation..."
kill $QSPY_PID 2>/dev/null
kill $RENODE_PID 2>/dev/null
wait $QSPY_PID $RENODE_PID 2>/dev/null
echo "Simulation run completed!"
