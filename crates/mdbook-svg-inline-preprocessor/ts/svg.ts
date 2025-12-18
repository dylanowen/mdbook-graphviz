import Tabby from "tabbyjs";
import {setupSvg} from "./viewer";

// crates/mdbook-svg-inline-preprocessor/src/renderer.rs
const TAB_HEADER_ID_PREFIX = 'svg-tabs';

function setupTabby() {
    for (const tabsHeader of document.querySelectorAll(`[id^="${TAB_HEADER_ID_PREFIX}"]`)) {
        new Tabby(`#${tabsHeader.id}`);
    }
}

document.addEventListener('DOMContentLoaded', () => {
    setupSvg();
    setupTabby();
});
