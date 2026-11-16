{
  "general": { "model": "Sodick ALC400G", "vendor": "Sodick", "kind": "wire_edm",
    "description": "Sodick ALC400G wire EDM. Specs: Sodick ALC400G pages: X/Y 400 x 300 mm and U/V 150 x 150 mm (sodick.eu/products/wire-edm), taper +-25 deg at 130 mm work height (sodick.co.th/EN/EDW.php); the sources give lengths, so each travel is centred on zero. Limit: the taper figure is data only. This file states no guide heights, so a tapered cut is refused at post. The Sodick post writes a vertical 2-axis cut only, so it refuses every taper (#1800). Guide heights, AWT, wire diameters, conditions and codes have no public source and are left out" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "x", "name": "X", "type": "linear", "travel": { "min": -200.0, "max": 200.0 }, "parts": [
          { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -150.0, "max": 150.0 } }
        ] },
        { "id": "u", "name": "U", "type": "linear", "travel": { "min": -75.0, "max": 75.0 }, "parts": [
          { "id": "v", "name": "V", "type": "linear", "travel": { "min": -75.0, "max": 75.0 } }
        ] },
        { "id": "head", "name": "guides", "type": "head" }
      ] }
    ] } },
  "edm": {
    "max_taper_deg": 25.0
  }
}
