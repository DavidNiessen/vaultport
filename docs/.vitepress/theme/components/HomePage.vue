<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import CtaBand from './CtaBand.vue';
import FeatureBento from './FeatureBento.vue';
import HeroSection from './HeroSection.vue';
import PlatformStrip from './PlatformStrip.vue';
import PrivacySection from './PrivacySection.vue';
</script>

<template>
  <div class="home">
    <HeroSection />
    <PlatformStrip />
    <FeatureBento />
    <PrivacySection />
    <CtaBand />
  </div>
</template>

<style scoped>
.home {
  overflow-x: clip;
}
</style>
