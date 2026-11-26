{
  "general": { "model": "Tormach 1500MX", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -14.0, "max": 14.0 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -13.8, "max": 13.8 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -19.7, "max": 19.7 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 250.0, "max_speed": 10000.0, "power": 3.0,
            "rigid_tapping": true },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 1200.0, "max_rapid_speed": 1200.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 1200.0, "max_rapid_speed": 1200.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 600.0, "max_rapid_speed": 600.0 }
  } } },
  "tooling": { "spindle_mount": "BT30", "default_holder": "BT30 ER20",
    "mount_source": "https://www.tormach.com/tormach_mills/ - MX machines have a BT30 spindle and tool holder" }
}
