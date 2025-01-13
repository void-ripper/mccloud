<script setup lang="ts">
import { RouterLink, RouterView } from 'vue-router'
</script>

<template lang="pug">
header.container
  nav.navbar
    .navbar-brand
      .navbar-item.is-size-3 McCloud
    .navbar-menu
      .navbar-start
        RouterLink.navbar-item(to="/") Home

main.container
  RouterView
</template>

<style>

</style>
