&commat;echo off
SET script_dir="%~dp0"
set /p "streamid=Enter the stream ID to spectate: "
%script_dir%/swb spectate %streamid%
