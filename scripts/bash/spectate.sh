#!/bin/bash
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
read -p "Enter the stream ID to spectate: " streamid
$SCRIPT_DIR/swb spectate streamid
