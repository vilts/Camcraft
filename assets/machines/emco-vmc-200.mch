{
  "general": {
    "model": "EMCO VMC-200",
    "vendor": "Camcraft",
    "kind": "mill",
    "description": "EMCO VMC-200 3-axis VMC (Siemens Sinumerik 810M retrofit / WinNC 810M). Public EMCO VMC-200 specification handbook: X/Y/Z travels 420/330/240 mm, rapid 10.5 m/min, working feed to 4000 mm/min, stepless spindle 50-4500 RPM, SK40, 12-tool ATC carousel, flood coolant."
  },
  "kinematics": {
    "default": {
      "units": { "length": "mm", "angle": "degrees" },
      "parts": [
        { "id": "base", "name": "table", "type": "table", "parts": [
          { "id": "x_slide", "name": "X", "type": "linear", "travel": { "min": -420.0, "max": 420.0 }, "parts": [
            { "id": "y_saddle", "name": "Y", "type": "linear", "travel": { "min": -330.0, "max": 330.0 } }
          ] },
          { "id": "column", "name": "Z", "type": "linear", "travel": { "min": -240.0, "max": 240.0 }, "safe_retract": 0.0, "parts": [
            { "id": "head", "name": "head", "type": "head",
              "spindle": { "min_speed": 50.0, "max_speed": 4500.0 },
              "tool_station": {
                "coolants": [
                  { "name": "Flood", "mCode": "M8" }
                ]
              }
            }
          ] }
        ] }
      ]
    }
  },
  "controller": {
    "post": "Siemens 810M",
    "synced_configuration": {
      "parts": {
        "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 10500.0 },
        "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 10500.0 },
        "Z": { "coordinate": 2, "max_normal_speed": 4000.0, "max_rapid_speed": 10500.0 }
      }
    }
  },
  "tooling": { "number_of_tools": 12 }
}
