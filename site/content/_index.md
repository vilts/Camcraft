+++
title = "CamCraft"
sort_by = "weight"
template = "index.html"
+++

CamCraft is a CAM program: you import a 3D model, describe the stock, the
work offset and the tools, add machining operations, and post an NC program
(G-code) for your mill or lathe. It runs on Windows, Linux, macOS and
FreeBSD, and it is written in Rust.

Every toolpath is checked against the part, the stock, the fixtures and the
machine before it can be posted. When CamCraft cannot show that a program is
safe, it refuses to write it and says why.

> **Pre-1.0 software.** CamCraft changes quickly. Treat every posted program
> as a first draft: check it in your machine's own simulation and dry-run it
> with the spindle off and Z raised before cutting material.
