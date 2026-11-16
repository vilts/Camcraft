{
  "general": { "model": "Makera Z1", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -100.0, "max": 100.0 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -200.0, "max": 200.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -200.0, "max": 200.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 13000.0, "power": 0.15 },
          "tool_station": {
            "coolants": [
              { "name": "Air", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 1200.0, "max_rapid_speed": 1200.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 1200.0, "max_rapid_speed": 1200.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 1200.0, "max_rapid_speed": 1200.0 }
  } } }
}
