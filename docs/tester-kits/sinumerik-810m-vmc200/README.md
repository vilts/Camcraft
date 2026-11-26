# EMCO VMC-200 + Siemens Sinumerik 810M Air-Cut Kit

**Machine:** EMCO VMC-200 (12-tool SK40 ATC, 50-4500 RPM, X 420 / Y 330 / Z 240 mm)
**Controller:** Siemens Sinumerik 810M GA3 (or WinNC Sinumerik 810M/820M)
**Post:** `assets/posts/sinumerik-810m.cps` (`Siemens 810M`)
**Built by CamCraft:** `air_cut_vmc200.camproj` generates `air_cut_vmc200.mpf`, pinned by `crates/app/tests/sinumerik_810m_post.rs`.

## Program Summary
- `Perimeter Skim` — T1 (`6mm flat end mill`, `D1`), 2500 RPM, 600 mm/min, G54, shallow 1 mm perimeter contour around a `60 x 40 x 20 mm` test block (`Z20 -> Z19`).
- `Peck Pair` — T2 (`5mm drill`, `D2`), 1800 RPM, 180 mm/min, two peck-drilled holes (`L83 P1` with `R00..R11`) at `(20, 20)` and `(40, 20)`, depth `Z14` from top `Z20` (`R02=20`, `R03=14`, `R01=3`, `R10=24`, `R11=0`).

## RS-232 Transfer Notes
- The file opens directly with `%MPF1` and ends with `M30` (LF line endings, ASCII uppercase, `( ... )` comments, max 120 chars/block, `N` numbers omitted by default to save 810M memory).
- Standard 810M serial defaults: XON/XOFF handshaking; check machine data (`MD`) for baud rate (typically 9600, 8N2 or 7E2).

## Safe Air-Cut Procedure (Read Back & Dry Run)
1. **Mount T1 (6 mm end mill, geometry + radius in D1) and T2 (5 mm drill, geometry in D2)**, or edit `T1`/`D1` and `T2`/`D2` to match your tool table.
2. **Load `%MPF1` (`air_cut_vmc200.mpf`)** over RS-232 and check the 810M screen for zero load/syntax alarms.
3. **Shift G54 Z +50 mm above stock** (so Z0 sits 50 mm in clear air above the top face / vise) and set X0 Y0 over an empty corner of the table.
4. **Engage Single-Block (`SBL`) and turn Rapid + Feed override down (10-25%).**
5. **Verify every key line during the air cut:**
   - Opening safe line: `G17 G71 G90 G94 G54 G40 G80`
   - Before each tool change and at end: `G0 G53 Z0 D0` retracts Z to machine zero cleanly before `T1 M6` / `T2 M6`, followed by `D1` / `D2`.
   - `Peck Pair` writes `R00..R11` and calls `L83 P1` (not `CYCLE83` or `;` comments) at both `(20, 20)` and `(40, 20)`, retracting to `R10=24` between holes.
   - Program ends cleanly with `M9`, `M5`, `G0 G53 Z0 D0`, `M30`.
6. Once the air cut passes cleanly, optionally restore G54 Z0 on test stock (wax/plastic/6061 aluminum) and run a real test cut.

Report back your 810M SW version (GA1/GA2/GA3), whether load/syntax ran without alarms (especially `T M6` / `D`, `L83 P1`, and `%MPF1`), and any OEM M-code quirks on your VMC-200.
