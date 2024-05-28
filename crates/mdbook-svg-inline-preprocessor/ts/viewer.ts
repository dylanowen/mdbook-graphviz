import svgPanZoom from "svg-pan-zoom";

// crates/mdbook-svg-inline-preprocessor/src/renderer.rs
const TAB_CONTENT_CLASS = 'svg-content';
const CAN_ZOOM_CLASS = 'svg-can-zoom';
const DISABLE_ZOOM_TIMEOUT = 5000;
const MIN_ENABLE_ZOOM_AFTER_SCROLL = 300;

const svgs: [SvgPanZoom.Instance, SVGSVGElement][] = [];
let zoomEnabled = false;
let zoomTimeout: ReturnType<typeof setTimeout> | null = null;

function enableZoom(): void {
    if (!zoomEnabled) {
        zoomEnabled = true;
        svgs.forEach(([svg, element]) => {
            element.classList.add(CAN_ZOOM_CLASS);
            svg.enableZoom();
        });

        if (zoomTimeout) {
            clearTimeout(zoomTimeout);
        }
        zoomTimeout = setTimeout(disableZoom, DISABLE_ZOOM_TIMEOUT);
    }
}

function disableZoom(): void {
    if (zoomEnabled) {
        zoomEnabled = false;
        svgs.forEach(([svg, element]) => {
            element.classList.remove(CAN_ZOOM_CLASS);
            svg.disableZoom()
        });
    }
}

export function setupSvgPanZoom() {
    for (const svgContent of document.querySelectorAll(`[id^="${TAB_CONTENT_CLASS}-"]`)) {
        const svg = svgContent.querySelector('svg');
        console.log(svg);
        if (svg) {
            svg.addEventListener('click', () => {
                if (!zoomEnabled) {
                    enableZoom();
                }
            });

            svgs.push([svgPanZoom(svg, {
                zoomScaleSensitivity: 0.3,
                zoomEnabled
            }), svg]);
        }
    }


    let lastScrolled = 0;
    document.addEventListener('mousemove', () => {
        // if we scrolled within a set time don't enable zoom
        if (Date.now() - lastScrolled > MIN_ENABLE_ZOOM_AFTER_SCROLL) {
            enableZoom();
        }
    });
    document.addEventListener('scroll', () => {
        disableZoom();
        lastScrolled = Date.now();
    });
}
