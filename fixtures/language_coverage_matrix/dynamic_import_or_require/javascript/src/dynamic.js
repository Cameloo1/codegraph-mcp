async function load(name) { return import('./plugins/' + name); }
function loadRequire(name) { return require(name); }
