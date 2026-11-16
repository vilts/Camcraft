{
  "general": { "model": "SINKER EDM SYNTHETIC DIALECT (FIXTURE)", "vendor": "Camcraft", "kind": "sinker_edm" },
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
  "edm": {
    "pickup": "synthetic holder",
    "orbits": [ { "kind": "circular", "word": "M150 R{radius}" } ],
    "conditions": [
      { "id": "S1", "name": "synthetic rough", "gap": 0.05, "undersize": 0.1, "role": "rough", "notes": "invented for tests - no manual" }
    ],
    "codes": {
      "metric": "G21",
      "inch": "G20",
      "pickup": "T{tool} M06",
      "reference": "M100 H{offset}",
      "condition": "E{id}",
      "depth": "M101 Z{z}",
      "end": "M02"
    }
  }
}
