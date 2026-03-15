// Renderer module - no header
export function render(store) {
    document.body.innerHTML = JSON.stringify(store.data);
}
