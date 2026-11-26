{
  "general": { "model": "Tormach 1300PL", "vendor": "Camcraft", "kind": "plasma" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "lifter", "name": "Z", "type": "linear", "travel": { "min": -3.9, "max": 3.9 }, "parts": [
          { "id": "gantry", "name": "Y", "type": "linear", "travel": { "min": -50.0, "max": 50.0 }, "parts": [
            { "id": "carriage", "name": "X", "type": "linear", "travel": { "min": -48.5, "max": 48.5 } }
          ] }
        ] },
        { "id": "torch", "name": "torch", "type": "head" }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot Plasma", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 1000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 1000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 120.0 }
  } } },
  "tooling": { "number_of_tools": 1 }
}
