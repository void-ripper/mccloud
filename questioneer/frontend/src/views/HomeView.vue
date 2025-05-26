<template lang="pug">

Modal(title="Connect To" :show="showConnect" @close="showConnect = false" @ok="performConnect")
    .columns.is-centered
        .column.is-three-quarters
            .select.is-multiple.is-fullwidth
                select(multiple size="8" v-model="toConnect").mono
                    option(v-for="peer in connectables" :value="peer.id") {{ peer.id }}


.columns.is-centered.mt-5
    .column.is-two
        .field.has-addons
            .control
                input.input(type="number" v-model="spawnCount")
            .control
                button.button.is-info(@click="onSpawn") spawn

        .field.has-addons
            .control
                input.input(type="text" v-model="msgToShare")
            .control
                button.button(@click="onShare()") share

        .field.has-addons
            .control
                input.input(type="number" step="1000" v-model="flakeTime")
            .control
                input.input(type="number" v-model="flakies")
            .control
                button.button(@click="onFlake()" :class="{'is-primary': isFlaking }") flake

        .buttons
            button.button(@click="onRefresh()") refresh
            button.button(@click="onConnect()") connect
            button.button(@click="onCircleConnect()") circle connect
            button.button(@click="onShutdownAll()") shutdown all
            button.button(@click="onShutdown()") shutdown

        .table-container.scroll
            table.table.is-narrow
                thead
                    tr
                      th peer ({{peerList.length}})
                tbody.mono
                    tr(v-for="ns in peerList")
                        td(@click="onClick(ns)" :class="{'is-selected': ns.id == target.id}") {{ ns.id }}

        .columns
            .column
                .box.content
                    h5 all known [{{ target.all_known.length }}]
                    .scroll
                        ul.mono
                            li(v-for="id in target.all_known") {{ id.substring(0, 12) }}
                .box.content
                    h5 connections [{{ target.connections.length }}]
                    .scroll
                        ul.mono
                            li(v-for="id in target.connections") {{ id.substring(0, 12) }}

            .column
                .box.content
                    h5 blocks
                    ol.mono
                      li(v-for="blk in blocks") {{ blk.hash.substring(0, 12) }}: {{ blk.author.substring(0, 12) }}
                        ol
                          li(v-for="n in blk.next_authors") {{ n.substring(0, 12) }}
                        ul
                          li(v-for="d in blk.data") {{ d }}
    .column
        LayoutD3(:peers="peerList" @pick="onPick")
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import LayoutD3 from "@/components/LayoutD3.vue";
import Modal from "@/components/Modal.vue";
import type { Peer } from "@/api";

const spawnCount = ref(1);
const peerList = ref([] as Peer[]);
const showConnect = ref(false);
const target = ref<Peer>({ id: "", connections: [], all_known: [] });
const toConnect = ref([] as string[]);
const msgToShare = ref("");
const flakies = ref(1);
const flakeTime = ref(30000);
const isFlaking = ref(false);
const flakeInterval = ref(0);
const blocks = ref([]);

const connectables = computed(() => {
    return peerList.value
        .filter((n) => n.id !== target.value.id)
        .filter((n) => target.value.connections.indexOf(n.id) === -1);
});

onMounted(async () => {
    await list();
});

function onPick(id: string) {
    target.value = peerList.value.find((p) => p.id === id)!;
    onBlocks();
}

function onClick(peer: Peer) {
    target.value = peer;
    onBlocks();
}

async function list() {
    const resp = await fetch("/api/list");
    peerList.value = await resp.json();
}

async function onRefresh() {
    await list();
}

async function onSpawn() {
    const resp = await fetch("/api/create", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            thin: false,
            count: spawnCount.value,
        }),
    });
    peerList.value = await resp.json();
}

async function onShutdown() {
    const resp = await fetch("/api/shutdown/" + target.value.id, {
        method: "POST",
    });
    peerList.value = await resp.json();
}

async function onShutdownAll() {
    for (let p of peerList.value) {
        await fetch("/api/shutdown/" + p.id, { method: "POST" });
    }
    await list();
}

async function onShare() {
    const resp = await fetch("/api/share", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: target.value.id, msg: msgToShare.value }),
    });
    msgToShare.value = "";
    setTimeout(onBlocks, 300);
}

function onConnect() {
    showConnect.value = true;
}

async function performConnect() {
    showConnect.value = false;

    for (let to of toConnect.value) {
        const resp = await fetch("/api/connect", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ frm: target.value.id, to: to }),
        });
    }

    await list();
}

async function onCircleConnect() {
    showConnect.value = false;

    for (let i = 1; i < peerList.value.length; i++) {
        const from = peerList.value[i - 1];
        const to = peerList.value[i];
        const resp = await fetch("/api/connect", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ frm: from.id, to: to.id }),
        });
    }
    const from = peerList.value[0];
    const to = peerList.value[peerList.value.length - 1];
    const resp = await fetch("/api/connect", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ frm: from.id, to: to.id }),
    });

    await list();
}

async function onBlocks() {
    const resp = await fetch("/api/blocks/" + target.value.id, {
        method: "POST",
    });
    blocks.value = await resp.json();
}

async function onFlake() {
    isFlaking.value = !isFlaking.value;

    if (isFlaking.value) {
        flakeInterval.value = setInterval(async () => {
            for (let i = 0; i < flakies.value; i++) {
                const len = Math.floor(Math.random() * peerList.value.length);
                const p = peerList.value[len];
                await fetch("/api/shutdown/" + p.id, {
                    method: "POST",
                });
            }
            await list();

            const resp = await fetch("/api/create", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    thin: false,
                    count: flakies.value,
                }),
            });
            const newones = await resp.json();

            for (let n of newones) {
                const len = Math.floor(Math.random() * peerList.value.length);
                const p = peerList.value[len];

                await fetch("/api/connect", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ frm: n.id, to: p.id }),
                });
            }

            await list();
        }, flakeTime.value);
    } else {
        clearInterval(flakeInterval.value);
    }
}
</script>

<style scoped>
.mono {
    font-family: monospace;
}

.scroll {
    max-height: 300px;
    overflow-y: scroll;
}
</style>
