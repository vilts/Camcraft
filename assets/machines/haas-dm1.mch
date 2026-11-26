{
  "general": { "model": "HAAS DM-1", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -394.0, "max": 394.0 }, "safe_retract": 0.0, "parts": [
          { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -508.0, "max": 508.0 }, "parts": [
            { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -406.0, "max": 406.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 10000.0, "power": 11.2, "rigid_tapping": true },
          "tool_station": { "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
      ] }
    ] } },
  "controller": { "post": "Haas", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 30480.0, "max_rapid_speed": 60960.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 30480.0, "max_rapid_speed": 60960.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 30480.0, "max_rapid_speed": 60960.0 }
  } } },
  "tooling": { "number_of_tools": 18, "spindle_mount": "CAT40", "default_holder": "CAT40 ER32" }
}
