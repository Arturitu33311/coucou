#!/usr/bin/env bash
# The files that install.sh copies into ~/bin. One list, read by install.sh and by `coucou-setup doctor`
# (which compares each against the deployed copy to report drift).
echo jinx_thread.py jinx_baseline.py jinx_chat.py jinx-chat coucou-dispatch jinx-health-guard jinx-voice-alert
