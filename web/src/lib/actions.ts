// Small behaviours attached to elements with `use:`.

/**
 * Marks a horizontally scrolling row whose content runs past an edge, so
 * CSS can fade that edge, and lets a mouse wheel scroll it sideways.
 */
export function overflowFade(node: HTMLElement): { destroy(): void } {
  const update = () => {
    const hidden = node.scrollWidth - node.clientWidth;
    node.classList.toggle("more-right", hidden - node.scrollLeft > 2);
    node.classList.toggle("more-left", node.scrollLeft > 2);
  };
  const wheel = (event: WheelEvent) => {
    if (node.scrollWidth <= node.clientWidth || Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
      return;
    }
    event.preventDefault();
    node.scrollLeft += event.deltaY;
  };
  const resize = new ResizeObserver(update);
  resize.observe(node);
  const mutation = new MutationObserver(update);
  mutation.observe(node, { childList: true, subtree: true, characterData: true });
  node.addEventListener("scroll", update, { passive: true });
  node.addEventListener("wheel", wheel, { passive: false });
  update();
  return {
    destroy() {
      resize.disconnect();
      mutation.disconnect();
      node.removeEventListener("scroll", update);
      node.removeEventListener("wheel", wheel);
    },
  };
}
