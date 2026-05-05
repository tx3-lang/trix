# Design 003 Assets

This folder contains PlantUML C4 architecture diagrams for the AI Aiken Vulnerability Scaffolding design.

## Files

- `c4-context.puml` - System context diagram
- `c4-container.puml` - Container-level architecture
- `c4-component.puml` - Component-level details

## Generating Images

### Using PlantUML CLI

Install PlantUML and run:

```bash
plantuml c4-*.puml
```

This will generate PNG files in the same directory.

### Using Docker

```bash
docker run --rm -v $(pwd):/data plantuml/plantuml:latest c4-*.puml
```

### Using Online Editor

1. Copy the content of any `.puml` file
2. Go to https://www.plantuml.com/plantuml/uml/
3. Paste and generate

### Using VS Code

Install the PlantUML extension:
- Extension ID: `jebbs.plantuml`
- Right-click on `.puml` file → "Preview Current Diagram"
- Export as PNG/SVG

## Output

Generated images (`*.png`, `*.svg`) should be committed to this directory so they render in the markdown document on GitHub and other viewers.
