+++
title = "Windows troubleshooting"
description = "Try another graphics backend, check a freezing PC, and collect a useful problem report."
weight = 6
+++

## The app does not start or closes at once

Try another graphics backend: the route CamCraft uses to draw its window.
Close CamCraft, right-click its Windows shortcut, select **Properties**, and
edit **Target**. Keep the executable path in quotes and add the option after
the closing quote. For example, replacing the path with your installation:

```text
"C:\CamCraft\camcraft-ui.exe" --gpu-backend dx12
```

Click **Apply** and start that shortcut. Try one backend at a time:

- `--gpu-backend dx12` uses Direct3D 12, Windows' native graphics interface.
- `--gpu-backend vulkan` uses Vulkan, an alternative graphics interface
  provided by a compatible graphics driver.
- `--gpu-backend gl` uses OpenGL, another graphics interface that can help
  when a driver has trouble with the other backends.

These options do not repair a driver or guarantee that a backend works on
your PC. Record which ones start successfully. Remove the option to return
to automatic selection. For a named error message, see
[Troubleshooting](@/guide/reference/troubleshooting.md).

## The whole PC freezes or the screen shows “snow”

A frozen desktop or corrupted display can indicate a graphics driver or
hardware hang triggered while CamCraft draws its 3D view. This differs from
only the CamCraft window closing. The symptom alone cannot identify the
faulty component. One report involved a desktop Ryzen 5 with onboard Radeon
graphics; it does not establish that every similar PC has the same problem.

Save your work before testing. Try these checks in order, changing one
thing at a time:

1. Install the latest suitable graphics driver from
   [AMD](https://www.amd.com/en/support/download/drivers.html),
   [Intel](https://www.intel.com/content/www/us/en/download-center/home.html),
   or [NVIDIA](https://www.nvidia.com/en-us/drivers/), matching your graphics
   hardware and Windows version. Restart and try again.
2. Onboard graphics share the PC's main RAM. If your BIOS has an **XMP** or
   **EXPO** memory profile enabled, try it off to test at the standard RAM
   settings. Record the original setting so you can restore it. If the
   motherboard offers **UMA Frame Buffer Size**, its usual **Auto** setting
   is the starting point. Only try a manual 2–4 GB allocation if the board
   maker supports it and you have enough RAM; available options and suitable
   sizes depend on installed RAM. This is a diagnostic trial, not a proven
   freeze fix. It reserves RAM for graphics and leaves less for Windows. See
   [AMD's UMA guidance](https://www.amd.com/en/resources/support-articles/faqs/PA-280.html).
3. Check the PC or motherboard maker's support page for a BIOS update.
   Follow that maker's instructions for your exact model and keep power
   connected throughout the update.
4. Check CPU and graphics temperatures against the hardware maker's limits.
   Stop testing if temperatures climb beyond those limits. Check cooling
   and seek hardware support if the PC remains unstable.

## Testing the PC itself

Save work and close other applications first. Stress tests can freeze or
restart an unstable PC. These tools have free editions for personal use;
they are named as diagnostic options, not endorsed by CamCraft:

| Tool | What it checks |
| --- | --- |
| [MemTest86](https://www.memtest86.com/) | Tests RAM after booting from a USB drive. Follow its USB setup instructions; writing the test image erases the selected USB drive. |
| [OCCT](https://www.ocbase.com/occt) | Tests CPU, RAM and graphics stability, with temperature monitoring. Run one test at a time. |
| [HWiNFO64](https://www.hwinfo.com/) | Shows hardware sensors, including CPU and graphics temperatures; it does not itself stress-test them. |

A passing test does not prove that every component or driver is sound.
Record errors and temperatures rather than repeatedly forcing a freezing
PC through the same test.

## What to send us

Include the CamCraft **version**, your Windows version, which backend you
tried, and the exact step where it failed. Say whether only CamCraft closed
or the entire PC stopped responding. Include the error text and a small
project that reproduces it, if you can share one.

Press **Windows+R**, type `dxdiag`, press **Enter**, then choose **Save All
Information**. The report describes your graphics hardware and drivers.
It includes the **PC name**. Read the report before sharing and remove
personal information you do not want to publish.

This version writes diagnostics to the launching terminal; it does not yet
save an automatic persistent log file. To capture a log, open **Command
Prompt** (`cmd.exe`) and run the following, using your executable's path:

```bat
cd /d "%USERPROFILE%"
"C:\CamCraft\camcraft-ui.exe" --gpu-backend dx12 2> camcraft-log.txt
```

Reproduce the problem and close CamCraft. The file is
`%USERPROFILE%\camcraft-log.txt`, in your Windows user folder. This command
replaces an existing file of that name. Inspect it for personal information
before sharing. A whole-PC freeze may prevent the final messages being
written. Automatic file logging is tracked in
[#2421](https://github.com/NeoTech/Camcraft/issues/2421); its location will
need documenting when that feature ships.

Send the report and available log in a
[GitHub issue](https://github.com/NeoTech/Camcraft/issues) or the
[CamCraft Discord](https://discord.gg/bBWjkAgX5). Reports there may be public.

## Windows on ARM and virtual machines

The Windows release is an x64 build. Windows 11 on ARM runs it through x64
emulation, which adds overhead. Virtual machines also depend on the graphics
adapters and drivers they expose to Windows.

One reported Parallels ARM virtual machine ran CamCraft using a software
renderer, which draws on the CPU and was slower. This is a reported result
from that setup, not a compatibility guarantee for other VMs or ARM PCs.
Include the VM product, Windows version and graphics adapter in your report.
