# Dot Density Map Example

This example demonstrates how to create an interactive Dot Density map using SIVO. Dot Density maps represent regional counts (e.g., population) by randomly placing dots inside the specific boundary of the region.

## Purpose
The purpose of this example is to show how:
1. An SVG file containing regional paths can be parsed and used as the basis for a geographic visualization.
2. Scalar quantitative data can be mapped to a specific number of visual dots representing a ratio (e.g., 1 dot represents 100 people).
3. The `apply_dot_density` method correctly executes a point-in-polygon algorithm to ensure dots are only rendered inside the actual geometric bounds of each specific region.
4. An `overlay` panel can be configured to show the underlying values associated with the region when clicked or interacted with.

## Key Code Snippets

**1. Instantiating the Sivo Application**
We initialize the app using an inline SVG string that has explicit `<path>` elements with unique IDs representing different zones. We also set `default_panel_position="overlay"` so that interactive elements can display HTML content over the map when clicked.
```python
sivo_app = Sivo.from_string(
    svg_data,
    title="Population Density",
    subtitle="1 Dot = 100 People",
    default_panel_position="overlay"
)
```

**2. Adding Interactive Content**
We bind rich HTML content to each zone using the `sivo_app.map()` function. This is displayed in the overlay panel.
```python
sivo_app.map("zoneA", html="<p>Zone A: 12,000 people</p>")
sivo_app.map("zoneB", html="<p>Zone B: 35,000 people</p>")
sivo_app.map("zoneC", html="<p>Zone C: 20,000 people</p>")
```

**3. Applying Dot Density**
We map the dataset containing the population counts to the regions and call `apply_dot_density()`. We define the size, color, and density scale of the dots. SIVO handles the path parsing and dot positioning inside the paths.
```python
sivo_app.apply_dot_density(
    data_map=data,
    dot_size=3.0,
    dot_color="rgba(37, 99, 235, 0.7)", # Blue dots
    dots_per_value=1 / 100 # 1 dot represents 100 people
)
```

## Running the Example
Execute the Python script to generate the interactive `output.html` file:
```bash
PYTHONPATH=src python3 examples/maps/dot_density_map/main.py
```
