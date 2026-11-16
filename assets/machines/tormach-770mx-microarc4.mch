{
  "general": { "model": "Tormach 770MX + microARC 4", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -13.25, "max": 13.25 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -7.5, "max": 7.5 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -14.0, "max": 14.0 } }
          ] }
        ] },
        { "id": "arc4", "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0],
          "travel": { "min": -360.0, "max": 360.0 }, "nose_to_axis": 10.6,
          "axis_to_table": 3.15 },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 175.0, "max_speed": 10000.0, "power": 1.1,
            "ranges": [{ "min_speed": 175.0, "max_speed": 3250.0 }, { "min_speed": 525.0, "max_speed": 10000.0 }],
            "rigid_tapping": true },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 300.0, "max_rapid_speed": 300.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 300.0, "max_rapid_speed": 300.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 250.0, "max_rapid_speed": 250.0 },
    "A": { "coordinate": 3 }
  } } },
  "tooling": { "number_of_tools": 10, "spindle_mount": "BT30", "default_holder": "BT30 ER20",
    "mount_source": "https://www.tormach.com/tormach_mills/ - MX machines have a BT30 spindle and tool holder" }
}
