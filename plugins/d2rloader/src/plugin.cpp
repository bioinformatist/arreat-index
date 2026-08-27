#include "observation.h"

#include <D2RLPlugin/context.h>
#include <D2RLPlugin/item.h>
#include <D2RLPlugin/lifecycle.h>
#include <D2RLPlugin/shared_events.h>
#include <D2RLPlugin/version.h>

#include <cstddef>
#include <cstdint>

namespace {

constexpr D2RL::PluginInfo kPluginInfo {
    .infoSize = D2RL::PluginInfoSize,
    .apiVersion = D2RL_PLUGIN_API_VERSION,
    .id = "arreat-index",
    .name = "Arreat Index Tooltip Identity Proof",
    .version = "0.1.0",
    .author = "Arreat Index contributors",
    .description = "Shows copied item identity facts in final item tooltips.",
    .flags = D2RL::PluginFlags::Client,
};

constexpr std::uint32_t kItemInfoFieldEnd = static_cast<std::uint32_t>(
    offsetof(D2RL::ItemServiceV1, getItemInfo) +
    sizeof(D2RL::Items::GetItemInfoFn));
constexpr std::uint32_t kRegisterTooltipFieldEnd = static_cast<std::uint32_t>(
    offsetof(D2RL::SharedEventServiceV1, registerItemTooltipListener) +
    sizeof(D2RL::SharedEvents::RegisterItemTooltipListenerFn));

[[nodiscard]] bool HasItemInspection(const D2RL::ItemServiceV1* service) noexcept {
    return D2RL::HasItemServiceV1Field(service, kItemInfoFieldEnd) &&
           service->getItemInfo != nullptr;
}

[[nodiscard]] bool HasTooltipRegistration(
    const D2RL::SharedEventServiceV1* service) noexcept {
    return D2RL::HasSharedEventServiceV1Field(service, kRegisterTooltipFieldEnd) &&
           service->registerItemTooltipListener != nullptr;
}

[[nodiscard]] arreat::d2rloader::ObservationQuality TranslateQuality(
    D2RL::Items::Quality quality) noexcept {
    switch (quality) {
        case D2RL::Items::Quality::Normal:
            return arreat::d2rloader::ObservationQuality::Normal;
        case D2RL::Items::Quality::Unique:
            return arreat::d2rloader::ObservationQuality::Unique;
        case D2RL::Items::Quality::Set:
            return arreat::d2rloader::ObservationQuality::Set;
        default:
            return arreat::d2rloader::ObservationQuality::Other;
    }
}

void __cdecl OnItemTooltip(
    const D2RL::PluginContext* context,
    D2RL::SharedEvents::ItemTooltipEvent* event,
    void*) noexcept {
    if (!D2RL::HasContext(context) || event == nullptr ||
        event->structSize < D2RL::SharedEvents::ItemTooltipEventRequiredSize ||
        event->text == nullptr || event->capacity == 0) {
        return;
    }
    event->text[0] = '\0';
    event->length = 0;

    const D2RL::ItemServiceV1* items = nullptr;
    if (context->QueryService(
            D2RL::ServiceId::Item,
            D2RL::ItemServiceV1Version,
            &items) != D2RL::ServiceQueryResult::Success ||
        !HasItemInspection(items)) {
        return;
    }

    D2RL::Items::ItemInfo info {
        .structSize = D2RL::Items::ItemInfoSize,
    };
    if (items->getItemInfo(context, event->item, &info) !=
        D2RL::Items::Result::Success) {
        return;
    }

    const arreat::d2rloader::ItemObservation observation {
        .code = info.code,
        .quality = TranslateQuality(info.quality),
        .quality_row = info.qualityRecordId,
        .quantity = info.quantity,
    };
    const std::size_t written = arreat::d2rloader::FormatTooltipObservation(
        observation,
        event->text,
        event->capacity);
    if (written != 0) {
        event->length = static_cast<std::uint32_t>(written);
    }
}

}  // namespace

D2RL_PLUGIN_EXPORT auto D2RLoaderGetPluginInfo() noexcept
    -> const D2RL::PluginInfo* {
    return &kPluginInfo;
}

D2RL_PLUGIN_EXPORT auto D2RLoaderLoadPlugin(
    const D2RL::PluginContext* context) noexcept -> bool {
    if (!D2RL::HasContext(context)) {
        return false;
    }

    const D2RL::SharedEventServiceV1* events = nullptr;
    const D2RL::ItemServiceV1* items = nullptr;
    if (context->QueryService(
            D2RL::ServiceId::SharedEvent,
            D2RL::SharedEventServiceV1Version,
            &events) != D2RL::ServiceQueryResult::Success ||
        !HasTooltipRegistration(events) ||
        context->QueryService(
            D2RL::ServiceId::Item,
            D2RL::ItemServiceV1Version,
            &items) != D2RL::ServiceQueryResult::Success ||
        !HasItemInspection(items)) {
        return false;
    }

    const D2RL::SharedEvents::ItemTooltipListener listener {
        .structSize = D2RL::SharedEvents::ItemTooltipListenerSize,
        .region = D2RL::SharedEvents::ItemTooltipRegion::Description,
        .position = D2RL::SharedEvents::ItemTooltipPosition::Bottom,
        .anchor = D2RL::SharedEvents::ItemTooltipAnchor::None,
        .fallback = D2RL::SharedEvents::ItemTooltipFallback::Omit,
        .callback = OnItemTooltip,
    };
    D2RL::SharedEvents::ListenerHandle handle =
        D2RL::SharedEvents::InvalidHandle;
    return events->registerItemTooltipListener(context, &listener, &handle) ==
               D2RL::SharedEvents::Result::Success &&
           handle != D2RL::SharedEvents::InvalidHandle;
}
