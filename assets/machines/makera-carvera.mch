{
  "general": { "model": "Makera Carvera", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -140.0, "max": 140.0 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -240.0, "max": 240.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -360.0, "max": 360.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 15000.0, "power": 0.2 },
          "tool_station": {
            "coolants": [
              { "name": "Air", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "post": "Makera Carvera", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 6000.0, "max_rapid_speed": 6000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 6000.0, "max_rapid_speed": 6000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 6000.0, "max_rapid_speed": 6000.0 }
  } } },
  "tooling": { "number_of_tools": 6 }
}
