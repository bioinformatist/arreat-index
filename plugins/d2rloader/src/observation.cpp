#include "observation.h"

#include <cstdio>
#include <cstring>

namespace arreat::d2rloader {
namespace {

constexpr std::size_t kCodeSize = 4;
constexpr std::size_t kProofLineCapacity = 96;

[[nodiscard]] bool DecodeItemCode(
    std::uint32_t packed,
    char (&code)[kCodeSize + 1],
    std::size_t& length) noexcept {
    for (std::size_t index = 0; index < kCodeSize; ++index) {
        const auto byte = static_cast<unsigned char>(packed >> (index * 8U));
        if (byte < 0x20U || byte > 0x7eU) {
            return false;
        }
        code[index] = static_cast<char>(byte);
    }

    length = kCodeSize;
    while (length > 0 && code[length - 1] == ' ') {
        --length;
    }
    if (length == 0) {
        return false;
    }
    for (std::size_t index = 0; index < length; ++index) {
        if (code[index] == ' ') {
            return false;
        }
    }
    code[length] = '\0';
    return true;
}

[[nodiscard]] bool IsRune(const ItemObservation& observation) noexcept {
    if (observation.quality != ObservationQuality::Normal ||
        observation.quantity <= 0) {
        return false;
    }

    const auto first = static_cast<unsigned char>(observation.code);
    const auto second = static_cast<unsigned char>(observation.code >> 8U);
    const auto third = static_cast<unsigned char>(observation.code >> 16U);
    const auto fourth = static_cast<unsigned char>(observation.code >> 24U);
    if (first != 'r' || fourth != ' ' || second < '0' || second > '3' ||
        third < '0' || third > '9') {
        return false;
    }

    const int rune_number = (second - '0') * 10 + (third - '0');
    return rune_number >= 1 && rune_number <= 33;
}

}  // namespace

std::size_t FormatTooltipObservation(
    const ItemObservation& observation,
    char* output,
    std::size_t capacity) noexcept {
    if (output != nullptr && capacity > 0) {
        output[0] = '\0';
    }
    if (output == nullptr || capacity == 0) {
        return 0;
    }

    char line[kProofLineCapacity] {};
    int length = 0;
    if (IsRune(observation)) {
        const auto second =
            static_cast<char>((observation.code >> 8U) & 0xffU);
        const auto third =
            static_cast<char>((observation.code >> 16U) & 0xffU);
        length = std::snprintf(
            line,
            sizeof(line),
            "Arreat Index proof: base:r%c%c owned=%d",
            second,
            third,
            observation.quantity);
    } else if (
        (observation.quality == ObservationQuality::Unique ||
         observation.quality == ObservationQuality::Set) &&
        observation.quality_row >= 0) {
        char code[kCodeSize + 1] {};
        std::size_t code_length = 0;
        if (!DecodeItemCode(observation.code, code, code_length)) {
            return 0;
        }
        const char* kind = observation.quality == ObservationQuality::Unique
                               ? "unique"
                               : "set";
        length = std::snprintf(
            line,
            sizeof(line),
            "Arreat Index proof: %s code=%.*s row=%d",
            kind,
            static_cast<int>(code_length),
            code,
            observation.quality_row);
    } else {
        return 0;
    }

    if (length <= 0 || static_cast<std::size_t>(length) >= sizeof(line)) {
        return 0;
    }
    const auto written = static_cast<std::size_t>(length);
    if (written >= capacity) {
        return 0;
    }

    std::memcpy(output, line, written + 1);
    return written;
}

}  // namespace arreat::d2rloader
