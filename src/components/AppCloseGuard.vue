<script setup lang="ts">
import { useAppCloseGuard } from '../composables/useAppCloseGuard'

// This owner lives under the message provider and survives every route change.
const { updateExit, confirmingClose } = useAppCloseGuard()
</script>

<template><slot :update-exit="updateExit" :confirming-close="confirmingClose" /></template>
