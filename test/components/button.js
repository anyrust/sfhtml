export class Button {
    constructor(el) { this.el = el; }
    onClick(fn) { this.el.addEventListener("click", fn); }
}
