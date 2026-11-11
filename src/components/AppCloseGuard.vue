<script setup lang="ts">
import { useAppCloseGuard } from '../composables/useAppCloseGuard'

// This owner lives under the message provider and survives every route change.
useAppCloseGuard()
</script>

<template></template>
