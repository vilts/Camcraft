{
  "general": { "model": "Sodick ALC400G", "vendor": "Sodick", "kind": "wire_edm",
    "description": "Sodick ALC400G wire EDM. Specs: Sodick ALC400G pages: X/Y 400 x 300 mm and U/V 150 x 150 mm (sodick.eu/products/wire-edm), taper +-25 deg at 130 mm work height (sodick.co.th/EN/EDW.php); the sources give lengths, so each travel is centred on zero. Limit: the taper figure is data only. The Sodick post writes a vertical 2-axis cut only, so it refuses every taper (#1800). EXAMPLE STARTER SET (#3214): the conditions C110 / C120 / C130, the guide heights and AWT are examples so that a first Wire Contour generates and posts - they are not Sodick figures; replace them with the rows of your control's condition table and your machine's guide heights before you cut. Wire diameters and codes have no public source and are left out" },
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
    "lower_guide_z": 0.0, "upper_guide_z": 130.0,
    "max_taper_deg": 25.0,
    "awt": true,
    "conditions": [
      { "id": "C110", "name": "Rough cut (example)", "role": "rough", "gap": 0.04, "wire_diameter": 0.25,
        "notes": "EXAMPLE - not a Sodick figure; replace with the row of your control's condition table" },
      { "id": "C120", "name": "Skim cut (example)", "role": "skim", "gap": 0.02, "wire_diameter": 0.25,
        "notes": "EXAMPLE - not a Sodick figure; replace with the row of your control's condition table" },
      { "id": "C130", "name": "Finish cut (example)", "role": "finish", "gap": 0.01, "wire_diameter": 0.25,
        "notes": "EXAMPLE - not a Sodick figure; replace with the row of your control's condition table" }
    ]
  }
}
