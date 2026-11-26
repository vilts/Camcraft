{
  "general": { "model": "Tormach 1100MX", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -16.25, "max": 16.25 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -11.0, "max": 11.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -18.0, "max": 18.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 70.0, "max_speed": 10000.0, "power": 1.5,
            "ranges": [{ "min_speed": 70.0, "max_speed": 2000.0 }, { "min_speed": 250.0, "max_speed": 10000.0 }],
            "rigid_tapping": true },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 300.0, "max_rapid_speed": 300.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 300.0, "max_rapid_speed": 300.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 230.0, "max_rapid_speed": 230.0 }
  } } },
  "tooling": { "spindle_mount": "BT30", "default_holder": "BT30 ER20",
    "mount_source": "https://www.tormach.com/tormach_mills/ - MX machines have a BT30 spindle and tool holder" }
}
