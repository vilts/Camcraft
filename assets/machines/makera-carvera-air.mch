{
  "general": { "model": "Makera Carvera Air", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -130.0, "max": 130.0 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -200.0, "max": 200.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -300.0, "max": 300.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 13000.0, "power": 0.2 },
          "tool_station": {
            "coolants": [
              { "name": "Air", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "post": "Makera Carvera", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 4000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 4000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 4000.0, "max_rapid_speed": 4000.0 }
  } } }
}
