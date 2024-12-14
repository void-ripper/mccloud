<template lang="pug">
.modal(:class="{'is-active': show}")
    .modal-background(@click="emits('close')")
    .modal-card
        header.modal-card-head
            p.modal-card-title {{ title }}
        section.modal-card-body
            slot
        footer.modal-card-foot
            .buttons
                button.button.is-success(@click="emits('ok')") ok
                button.button(@click="emits('close')") cancel
</template>

<script setup lang="ts">
import { ref } from 'vue';

const props = defineProps<{
    show: boolean,
    title: string,
}>()

const emits = defineEmits(["close", "ok"])
</script>