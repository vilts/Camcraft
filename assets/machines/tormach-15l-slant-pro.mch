{
  "general": { "model": "Tormach 15L Slant-PRO", "vendor": "Camcraft", "kind": "lathe" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "bed", "type": "table", "parts": [
        { "id": "cross", "name": "X", "type": "linear", "travel": { "min": -10.0, "max": 10.0 }, "parts": [
          { "id": "car", "name": "Z", "type": "linear", "travel": { "min": -12.0, "max": 12.0 } }
        ] },
        { "id": "ms", "name": "head", "type": "head",
          "spindle": { "min_speed": 180.0, "max_speed": 2500.0, "power": 2.24,
            "body_diameter": 0.0, "body_length": 0.0, "rigid_tapping": true },
          "tool_station": {
            "coolants": [ { "name": "Flood", "mCode": "M8" } ] } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot Lathe", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 60.0, "max_rapid_speed": 60.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 60.0, "max_rapid_speed": 60.0 }
  } } },
  "tooling": { "number_of_tools": 8 }
}
