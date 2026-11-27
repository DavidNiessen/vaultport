<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import CtaBand from './CtaBand.vue';
import FeatureBento from './FeatureBento.vue';
import FeatureMarquee from './FeatureMarquee.vue';
import HeroSection from './HeroSection.vue';
import HowItWorks from './HowItWorks.vue';
import PrivacySection from './PrivacySection.vue';
import SiteTicker from './SiteTicker.vue';
</script>

<template>
  <div class="home">
    <SiteTicker />
    <HeroSection />
    <FeatureMarquee />
    <HowItWorks />
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
