{
  "name": "{{project_name}}",
  "version": "{{protocol_version}}",
  "description": "A simple Node.js library for the TX3 protocol",
  "main": "dist/{{project_name}}.js",
  "types": "dist/{{project_name}}.d.ts",
  "scripts": {
    "build": "tsc",
    "test": "tsx ./test"
  },
  "dependencies": {
    "tx3-trp": "^0.2.0"
  },
  "devDependencies": {
    "@types/node": "^22.14.1",
    "tsx": "^4.19.3",
    "typescript": "^5.8.3"
  }
}
