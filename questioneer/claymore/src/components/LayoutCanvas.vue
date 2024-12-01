<template lang="pug">
canvas(ref="layout" width="800" height="600")
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue';
import type {Peer} from "@/api"

interface Node {
    id: string
    short: string
    x: number
    y: number
    vx: number
    vy: number
    marked: boolean
    connections: Node[]
    peer: Peer
}

const props = defineProps<{
    nodes: Peer[]
}>()
const layout = ref<HTMLCanvasElement | null>(null)
let realNodes = [] as Node[]
let afHandle = 0

function nodeFromPeer(p: Peer): Node {
    return {
        id: p.id,
        short: p.id.substring(0, 7),
        x: 800 * Math.random(),
        y: 600 * Math.random(),
        vx: 0,
        vy: 0,
        marked: false,
        connections: [],
        peer: p,
    }
}

function buildConnections(n: Node) {
    n.connections = realNodes.filter(x => n.peer.connections.indexOf(x.id) !== -1)
}

watch(() => props.nodes.length, () => {
    const newones = []

    for (let r of realNodes) {
        r.marked = false
    }

    for (let u of props.nodes) {
        let there = false
        for (let r of realNodes) {
            if (r.id === u.id) {
                r.marked = true
                there = true
                break
            }
        }

        if (!there) {
            newones.push(u)
        }
    }

    realNodes = realNodes.filter(n => n.marked)
    for (let n of newones) {
        realNodes.push(nodeFromPeer(n))
    }

    realNodes.map(buildConnections)
    console.log(realNodes)
})

onMounted(() => {
    const ctx = layout.value!.getContext("2d")
    const width = layout.value!.width
    const height = layout.value!.height

    ctx?.clearRect(0, 0, width, height)
    ctx?.strokeRect(0, 0, width, height)

    realNodes = props.nodes.map(nodeFromPeer)
    realNodes.map(buildConnections)
    console.log("init", realNodes)

    let start = 1000
    const size = 50
    const speed = 0.01
    const draw = (now: number) => {
        const del = (now - start) / 1000
        start = now

        ctx?.clearRect(0, 0, width, height)
        ctx?.strokeRect(0, 0, width, height)

        for (let n of realNodes) {
            let xd = n.x - width / 2
            let yd = n.y - height / 2

            for (let p of realNodes) {
                if (n !== p) {
                    const connected = n.connections.includes(p)
                    const dx = n.x - p.x
                    const dy = n.y - p.y
                    const d = Math.sqrt(dx * dx + dy * dy)

                    // if (Math.abs(d - size) > 0.5) {
                        let f = connected ? 1.5 : 1
                        if (d < size) {
                            f *= -realNodes.length
                        }
                        xd += dx * f
                        yd += dy * f
                    // }
                }
            }

            n.vx = xd
            n.vy = yd
        }

        for(let n of realNodes) {
            for(let p of n.connections) {
                ctx?.beginPath()
                ctx?.moveTo(n.x, n.y)
                ctx?.lineTo(p.x, p.y)
                ctx?.stroke()
            }
        }

        for (let n of realNodes) {
            n.x -= n.vx * speed
            n.y -= n.vy * speed

            ctx!.fillStyle = "white"
            ctx?.beginPath()
            ctx?.arc(n.x, n.y, 20, 0, Math.PI * 2)
            ctx?.fill()

            ctx?.beginPath()
            ctx?.arc(n.x, n.y, 20, 0, Math.PI * 2)
            ctx?.stroke()

            ctx!.fillStyle = "black"
            ctx?.fillText(n.short, n.x - 20, n.y + 2)
        }

        window.requestAnimationFrame(draw)
    }
    window.requestAnimationFrame(draw)
})

onUnmounted(() => {
})
</script>@/api