{
  "general": { "model": "TESTBENCH MILL-TURN LIVE", "vendor": "Camcraft", "kind": "mill-turn" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "car", "name": "X", "type": "linear", "travel": { "min": 0.0, "max": 650.0 }, "parts": [
          { "id": "sld", "name": "Y", "type": "linear", "travel": { "min": -100.0, "max": 100.0 }, "parts": [
            { "id": "quill", "name": "Z", "type": "linear", "travel": { "min": -620.0, "max": 620.0 } }
          ] }
        ] },
        { "id": "tn", "name": "C", "type": "rotary", "axis": [0.0, 0.0, 1.0],
          "travel": { "min": -360.0, "max": 360.0 }, "resolution": 0.001, "parts": [
            { "id": "head", "name": "head", "type": "head",
              "spindle": { "min_speed": 30.0, "max_speed": 10000.0, "power": 18.5,
                           "body_diameter": 90.0, "body_length": 70.0 },
              "tool_station": { "max_tool_length": 250.0, "max_tool_diameter": 80.0,
                "coolants": [
                  { "name": "Flood", "mCode": "M8" },
                  { "name": "Through-spindle", "mCode": "M88" }
                ],
                "live_tool": { "c_axis_on": "M1001", "c_axis_off": "M1002",
                               "spindle_on": "M1003", "spindle_off": "M1004" } } }
          ] }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 30000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 30000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 4000.0, "max_rapid_speed": 24000.0 },
    "C": { "coordinate": 3, "max_normal_speed": 6000.0, "max_rapid_speed": 12000.0 }
  } } },
  "tooling": { "number_of_tools": 40 }
}
