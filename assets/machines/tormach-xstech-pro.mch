{
  "general": { "model": "Tormach xsTECH Pro", "vendor": "Camcraft", "kind": "router" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -6.3, "max": 6.3 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -7.9, "max": 7.9 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -10.2, "max": 10.2 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "max_speed": 20000.0, "power": 0.35,
            "rigid_tapping": false } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot", "synced_configuration": { "parts": {
    "X": { "coordinate": 0 },
    "Y": { "coordinate": 1 },
    "Z": { "coordinate": 2 }
  } } },
  "tooling": { "number_of_tools": 4 }
}
