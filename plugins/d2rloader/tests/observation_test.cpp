#include "observation.h"

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>

namespace {

using arreat::d2rloader::FormatTooltipObservation;
using arreat::d2rloader::ItemObservation;
using arreat::d2rloader::ObservationQuality;

constexpr std::uint32_t PackCode(
    char first,
    char second,
    char third,
    char fourth = ' ') noexcept {
    return static_cast<std::uint32_t>(static_cast<unsigned char>(first)) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(second)) << 8U) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(third)) << 16U) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(fourth)) << 24U);
}

[[noreturn]] void Fail() noexcept {
    std::abort();
}

void Check(bool condition) noexcept {
    if (!condition) {
        Fail();
    }
}

void ExpectLine(const ItemObservation& observation, const char* expected) noexcept {
    char output[96] {};
    const std::size_t written =
        FormatTooltipObservation(observation, output, sizeof(output));
    Check(written == std::strlen(expected));
    Check(std::strcmp(output, expected) == 0);
}

void ExpectOmitted(const ItemObservation& observation) noexcept {
    char output[96] {'x', '\0'};
    Check(FormatTooltipObservation(observation, output, sizeof(output)) == 0);
    Check(output[0] == '\0');
}

}  // namespace

int main() {
    ExpectLine(
        {PackCode('r', '0', '1'), ObservationQuality::Normal, -1, 1},
        "Arreat Index proof: base:r01 owned=1");
    ExpectLine(
        {PackCode('r', '2', '6'), ObservationQuality::Normal, -1, 7},
        "Arreat Index proof: base:r26 owned=7");
    ExpectLine(
        {PackCode('r', '3', '3'), ObservationQuality::Normal, -1, 12},
        "Arreat Index proof: base:r33 owned=12");

    ExpectOmitted({PackCode('r', '0', '0'), ObservationQuality::Normal, -1, 1});
    ExpectOmitted({PackCode('r', '3', '4'), ObservationQuality::Normal, -1, 1});
    ExpectOmitted({PackCode('r', '2', '6'), ObservationQuality::Other, -1, 7});
    ExpectOmitted({PackCode('r', '2', '6'), ObservationQuality::Normal, -1, 0});
    ExpectOmitted({PackCode('r', '2', '6'), ObservationQuality::Normal, -1, -1});

    ExpectLine(
        {PackCode('c', 'a', 'p'), ObservationQuality::Unique, 42, 1},
        "Arreat Index proof: unique code=cap row=42");
    ExpectOmitted({PackCode('c', 'a', 'p'), ObservationQuality::Unique, -1, 1});
    ExpectLine(
        {PackCode('7', 'c', 'r'), ObservationQuality::Set, 17, 1},
        "Arreat Index proof: set code=7cr row=17");
    ExpectOmitted({PackCode('7', 'c', 'r'), ObservationQuality::Set, -1, 1});
    ExpectOmitted({PackCode('c', 'a', '\n'), ObservationQuality::Unique, 1, 1});

    constexpr char expected[] = "Arreat Index proof: base:r26 owned=7";
    char too_small[sizeof(expected) - 1] {'x'};
    Check(
        FormatTooltipObservation(
            {PackCode('r', '2', '6'), ObservationQuality::Normal, -1, 7},
            too_small,
            sizeof(too_small)) == 0);
    Check(too_small[0] == '\0');

    return 0;
}
