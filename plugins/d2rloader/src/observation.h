#pragma once

#include <cstddef>
#include <cstdint>

namespace arreat::d2rloader {

enum class ObservationQuality : std::uint8_t {
    Other,
    Normal,
    Unique,
    Set,
};

struct ItemObservation {
    std::uint32_t code;
    ObservationQuality quality;
    std::int32_t quality_row;
    std::int32_t quantity;
};

[[nodiscard]] std::size_t FormatTooltipObservation(
    const ItemObservation& observation,
    char* output,
    std::size_t capacity) noexcept;

}  // namespace arreat::d2rloader
