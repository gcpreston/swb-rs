echo off
SET script_dir="%~dp0"
SET /p "streamid=Enter the stream ID to spectate: "
%script_dir%\swb.exe spectate %streamid%
