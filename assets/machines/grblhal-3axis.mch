{
  "general": { "model": "grblHAL 3-axis", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -100.0, "max": 100.0 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -300.0, "max": 300.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -300.0, "max": 300.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 1000.0, "max_speed": 24000.0 },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 5000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 5000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 1500.0, "max_rapid_speed": 2000.0 }
  } } }
}
