<script setup lang="ts">
import { t } from '../i18n'
import type { TutorialProgressState } from '../tutorial/useTutorialRuntime'

defineProps<{ progress: TutorialProgressState }>()
const emit = defineEmits<{ cancel: [] }>()
</script>

<template>
  <Transition name="download-notch">
    <div v-if="progress.label" class="download-strip">
      <div class="download-meta"><span>{{ progress.label }}<small v-if="progress.status"> · {{ progress.status
      }}</small></span><span><template v-if="!progress.indeterminate">{{ progress.percent.toFixed(0) }}%</template><template v-if="progress.speed"> ·
      {{ progress.speed }}</template></span></div>
      <div class="download-progress-row">
        <div class="progress" :class="{ 'progress-indeterminate': progress.indeterminate }"><i :style="{ width: progress.indeterminate ? '28%' : `${progress.percent}%` }"></i></div><button
          class="secondary-button download-cancel" @click="emit('cancel')">{{ t('取消下载') }}</button>
      </div>
    </div>
  </Transition>
</template>
