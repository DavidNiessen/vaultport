<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import AppTour from './AppTour.vue';
import CipherPlayground from './CipherPlayground.vue';
import DecryptLens from './DecryptLens.vue';
import FeatureGrid from './FeatureGrid.vue';
import FeatureMarquee from './FeatureMarquee.vue';
import FinalCta from './FinalCta.vue';
import HeroSection from './HeroSection.vue';
import SyncFlow from './SyncFlow.vue';
import TrustList from './TrustList.vue';
import ZeroStats from './ZeroStats.vue';
</script>

<template>
  <div class="home">
    <HeroSection />
    <FeatureMarquee />
    <AppTour />
    <DecryptLens />
    <FeatureGrid />
    <SyncFlow />
    <CipherPlayground />
    <ZeroStats />
    <TrustList />
    <FinalCta />
  </div>
</template>

<style scoped>
.home {
  overflow-x: clip;
}
</style>
