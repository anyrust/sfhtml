<!DOCTYPE html>
<html>
<head>
<!-- AI-SKILL-HEADER START
# DataStore — Reactive data store module

## 1. Overview
ES module providing reactive data store for the app.

    AI-SKILL-HEADER END -->
</head>
<body>
<script type="module">
export class DataStore {
    constructor() { this.data = {}; }
    set(key, val) { this.data[key] = val; }
    get(key) { return this.data[key]; }
}
</script>
</body>
</html>
