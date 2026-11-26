{
  "general": { "model": "HAAS MINI MILL", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -381.0, "max": 381.0 }, "safe_retract": 0.0, "parts": [
          { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -406.0, "max": 406.0 }, "parts": [
            { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -356.0, "max": 356.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 8000.0, "power": 5.6, "rigid_tapping": true },
          "tool_station": { "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
      ] }
    ] } },
  "controller": { "post": "Haas", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 10160.0, "max_rapid_speed": 10160.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 10160.0, "max_rapid_speed": 10160.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 10160.0, "max_rapid_speed": 10160.0 }
  } } },
  "tooling": { "number_of_tools": 10, "spindle_mount": "CAT40", "default_holder": "CAT40 ER32" }
}
