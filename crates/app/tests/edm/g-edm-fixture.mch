{
  "general": { "model": "SYNTHETIC G-EDM WIRE (TEST FIXTURE)", "vendor": "Camcraft", "kind": "wire_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "x", "name": "X", "type": "linear", "parts": [
          { "id": "y", "name": "Y", "type": "linear" }
        ] },
        { "id": "head", "name": "guides", "type": "head" }
      ] }
    ] } },
  "edm": {
    "awt": false,
    "conditions": [
      { "id": "R1", "name": "synthetic rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough", "notes": "SYNTHETIC - invented numbers, no machine's manual" },
      { "id": "S2", "name": "synthetic skim", "gap": 0.02, "wire_diameter": 0.25, "role": "skim", "notes": "SYNTHETIC" }
    ]
  }
}
