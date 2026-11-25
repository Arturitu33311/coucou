# Glance (visual inspiration)

The unlock animation of the Android island (`coucou/ui/GlanceUnlock.kt`, `GlanceUnlockPill.kt`) takes its look and
timings from **Glance** by Jonny Oo, https://github.com/jonnyoo/glance, released under the MIT License:

    MIT License. Copyright (c) Jonny Oo. Permission is hereby granted, free of charge, to any person obtaining a copy of
    this software and associated documentation files (the "Software"), to deal in the Software without restriction,
    including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software ... The above copyright notice and this permission notice shall be included in all copies or
    substantial portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.

What was studied: the panel spring (response 0.45 s, damping 0.7; critically damped close), the 0.28 s tick with
cubic-bezier(0.65, 0, 0.35, 1), the 0.4 s lock open/close, the red failure with a shake, the 1.7 s success hold, and the
success/failure colours (#30D158 / #FF453A). No Swift code was copied; everything is rewritten for Compose.
