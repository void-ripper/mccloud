<template>
    <div ref="parent"></div>
</template>

<script setup lang="ts">
import * as d3 from "d3";
import type { Peer } from "@/api"
import { ref, watch } from "vue";

const width = 800
const height = 600
const parent = ref<HTMLDivElement | null>(null)
const props = defineProps<{
    peers: Peer[]
}>()
const emits = defineEmits(["pick"])
const color = d3.scaleOrdinal(d3.schemeCategory10);

const setup = () => {
    const nodes = props.peers.map(p => ({ id: p.id, x: 0.0, y: 0.0 }))
    const links = []

    for(let i = 0; i < props.peers.length; i++) {
        const p0 = props.peers[i]
        for(let id of p0.connections) {
            links.push({source: p0.id, target: id})
        }
    }

    /** @ts-ignore */
    const simulation = d3.forceSimulation(nodes)
    /** @ts-ignore */
        .force("link", d3.forceLink(links).id(d => d.id))
        .force("charge", d3.forceManyBody().strength(-100))
        .force("x", d3.forceX())
        .force("y", d3.forceY());

    const svg = d3.create("svg")
        .attr("width", width)
        .attr("height", height)
        .attr("viewBox", [-width / 2, -height / 2, width, height])
        .attr("style", "max-width: 100%; height: auto;");

    /** @ts-ignore */
    const link = svg.append("g")
        .attr("stroke", "#999")
        .attr("stroke-opacity", 0.6)
        .selectAll("line")
        .data(links)
        .join("line")
        // .attr("stroke-width", d => Math.sqrt(d.value));

    /** @ts-ignore */
    const node = svg.append("g")
        .attr("stroke", "#fff")
        .attr("stroke-width", 1.5)
        .selectAll("circle")
        .data(nodes)
        .join("circle")
        .attr("r", 10)
        .attr("fill", d => color(d.group));

    node.append("title")
        .text(d => d.id);

    // Add a drag behavior.
    /** @ts-ignore */
    node.call(d3.drag()
        .on("start", dragstarted)
        .on("drag", dragged)
        .on("end", dragended));

    // Set the position attributes of links and nodes each time the simulation ticks.
    /** @ts-ignore */
    simulation.on("tick", () => {
        link
    /** @ts-ignore */
            .attr("x1", d => d.source.x)
    /** @ts-ignore */
            .attr("y1", d => d.source.y)
    /** @ts-ignore */
            .attr("x2", d => d.target.x)
    /** @ts-ignore */
            .attr("y2", d => d.target.y);

        node
            .attr("cx", d => d.x)
            .attr("cy", d => d.y);
    });

    // Reheat the simulation when drag starts, and fix the subject position.
    /** @ts-ignore */
    function dragstarted(event) {
        if (!event.active) simulation.alphaTarget(0.3).restart();
        event.subject.fx = event.subject.x;
        event.subject.fy = event.subject.y;
        emits("pick", event.subject.id)
    }

    // Update the subject (dragged node) position during drag.
    /** @ts-ignore */
    function dragged(event) {
        event.subject.fx = event.x;
        event.subject.fy = event.y;
    }

    // Restore the target alpha so the simulation cools after dragging ends.
    // Unfix the subject position now that it’s no longer being dragged.
    /** @ts-ignore */
    function dragended(event) {
        if (!event.active) simulation.alphaTarget(0);
        event.subject.fx = null;
        event.subject.fy = null;
    }

    // When this cell is re-run, stop the previous simulation. (This doesn’t
    // really matter since the target alpha is zero and the simulation will
    // stop naturally, but it’s a good practice.)
    //   invalidation.then(() => simulation.stop());

    const el = svg.node();
    if (parent.value!.childElementCount > 0) {
        parent.value?.removeChild(parent.value.childNodes[0])
    }
    /** @ts-ignore */
    parent.value?.appendChild(el);

}

watch(() => props.peers.length + props.peers.reduce((acc, cur) => acc + cur.connections.length, 0), () => {
    setup()
})
// setTimeout(setup, 1000);
</script>