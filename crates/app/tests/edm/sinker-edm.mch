{
  "general": { "model": "SINKER EDM 3-AXIS (TEMPLATE)", "vendor": "Camcraft", "kind": "sinker_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "x", "name": "X", "type": "linear", "parts": [
          { "id": "y", "name": "Y", "type": "linear" }
        ] },
        { "id": "z", "name": "Z", "type": "linear", "parts": [
          { "id": "head", "name": "ram", "type": "head" }
        ] }
      ] }
    ] } },
  "edm": {}
}
