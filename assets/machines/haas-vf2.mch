{
  "general": { "model": "HAAS VF-2", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -508.0, "max": 508.0 }, "safe_retract": 0.0, "parts": [
          { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -762.0, "max": 762.0 }, "parts": [
            { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -406.0, "max": 406.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 8100.0, "power": 22.4, "rigid_tapping": true },
          "tool_station": { "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 16510.0, "max_rapid_speed": 25400.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 16510.0, "max_rapid_speed": 25400.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 16510.0, "max_rapid_speed": 25400.0 }
  } } },
  "tooling": { "number_of_tools": 20, "spindle_mount": "CAT40", "default_holder": "CAT40 ER32" }
}
