import svgPanZoom from "svg-pan-zoom";

declare global {
    // https://rust-lang.github.io/mdBook/format/theme/index-hbs.html?highlight=path_to_root
    const path_to_root: string;

    interface Window {
        mdbook_graphviz_svg_preprocessor: {
            svgs: MdBookSvg[];

        }
    }
}


class MdBookSvg {
    public readonly svg: SvgPanZoom.Instance;
    public readonly element: SVGSVGElement;

    constructor(svg: SvgPanZoom.Instance, element: SVGSVGElement) {
        this.svg = svg;
        this.element = element;
    }

    /**
     * Focus on an inner element of the SVG
     *
     * @param focusElement must be a child of a SVGGraphicsElement in the SVG
     * @param zoom the zoom level to apply
     */
    public focus(focusElement: Element, zoom: number = 2): void {
        let svgElement: Element | null = focusElement;
        while (svgElement && !(svgElement instanceof SVGGraphicsElement)) {
            svgElement = svgElement.parentElement;
        }
        if (svgElement) {
            // we might have an internal viewBox, search for it
            let internalViewBox = {x: 0, y: 0};
            let container: Element | null = svgElement;
            while (container && !(container instanceof SVGSVGElement)) {
                container = container.parentElement;
            }
            if (container) {
                const viewBox = container.getAttribute('viewBox');
                if (viewBox) {
                    const [x, y] = viewBox.split(' ').map(parseFloat);
                    internalViewBox = {x, y};
                }
            }

            const {x, y, width, height} = svgElement.getBBox();
            const {
                width: svgWidth,
                height: svgHeight,
                realZoom,
            } = this.svg.getSizes();
            const center = {x: x + width / 2, y: y + height / 2};

            // We're panning using user-space coordinates. Pan to the middle of the view then offset by the center of
            // our target
            const offset = {
                x: svgWidth * 0.5 + (-center.x + internalViewBox.x) * realZoom,
                y: svgHeight * 0.5 + (-center.y + internalViewBox.y) * realZoom
            };

            this.svg.pan(offset);
            this.svg.zoom(zoom);
        } else {
            throw new Error('focusElement is not a child of a SVGGraphicsElement');
        }
    }
}

// crates/mdbook-svg-inline-preprocessor/src/renderer.rs
const SVG_CONTENT_CLASS = 'svg-content';
const CAN_ZOOM_CLASS = 'svg-can-zoom';
const DISABLE_ZOOM_TIMEOUT = 5000;
const MIN_ENABLE_ZOOM_AFTER_SCROLL = 300;

const svgs: MdBookSvg[] = [];
//
window.mdbook_graphviz_svg_preprocessor = window.mdbook_graphviz_svg_preprocessor || {
    svgs
};
let zoomEnabled = false;
let zoomTimeout: ReturnType<typeof setTimeout> | null = null;

function enableZoom(): void {
    if (!zoomEnabled) {
        zoomEnabled = true;
        svgs.forEach(({svg, element}) => {
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
        svgs.forEach(({svg, element}) => {
            element.classList.remove(CAN_ZOOM_CLASS);
            svg.disableZoom()
        });
    }
}

export function setupSvg() {
    for (const svgContent of document.querySelectorAll(`[id^="${SVG_CONTENT_CLASS}-"]`)) {
        const svg = svgContent.querySelector('svg');
        if (svg) {
            // TODO make this configurable
            const shadowContainer = svg.parentElement!;

            const shadow = shadowContainer.attachShadow({mode: 'open'});
            const shadowStyle = document.createElement('link');
            shadowStyle.setAttribute('rel', 'stylesheet');
            shadowStyle.setAttribute('href', `${path_to_root}css/svg-shadow.css`);
            shadow.appendChild(shadowStyle);
            shadow.appendChild(svg)

            svg.addEventListener('click', () => {
                if (!zoomEnabled) {
                    enableZoom();
                }
            });

            svgs.push(new MdBookSvg(svgPanZoom(svg, {
                zoomScaleSensitivity: 0.3,
                zoomEnabled
            }), svg));
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

