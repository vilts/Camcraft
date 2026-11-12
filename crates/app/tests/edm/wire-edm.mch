{
  "general": { "model": "WIRE EDM 4-AXIS (TEMPLATE)", "vendor": "Camcraft", "kind": "wire_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "x", "name": "X", "type": "linear", "parts": [
          { "id": "y", "name": "Y", "type": "linear" }
        ] },
        { "id": "u", "name": "U", "type": "linear", "parts": [
          { "id": "v", "name": "V", "type": "linear" }
        ] },
        { "id": "head", "name": "guides", "type": "head" }
      ] }
    ] } },
  "edm": {}
}
