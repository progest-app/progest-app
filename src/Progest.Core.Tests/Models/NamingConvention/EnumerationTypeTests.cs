using FluentAssertions;
using Xunit;
using Progest.Core.Models.NamingConvention;

namespace Progest.Core.Tests.Models.NamingConvention;

public class EnumerationTypeTests
{
    [Fact]
    public void PrefixType_ShouldHaveThreeValues()
    {
        Enum.GetValues<PrefixType>().Length.Should().Be(3);
    }

    [Fact]
    public void PrefixType_ShouldContainAllTypes()
    {
        Enum.IsDefined(typeof(PrefixType), PrefixType.None).Should().BeTrue();
        Enum.IsDefined(typeof(PrefixType), PrefixType.Date).Should().BeTrue();
        Enum.IsDefined(typeof(PrefixType), PrefixType.Fixed).Should().BeTrue();
    }

    [Fact]
    public void SuffixType_ShouldHaveThreeValues()
    {
        Enum.GetValues<SuffixType>().Length.Should().Be(3);
    }

    [Fact]
    public void SuffixType_ShouldContainAllTypes()
    {
        Enum.IsDefined(typeof(SuffixType), SuffixType.None).Should().BeTrue();
        Enum.IsDefined(typeof(SuffixType), SuffixType.Version).Should().BeTrue();
        Enum.IsDefined(typeof(SuffixType), SuffixType.Fixed).Should().BeTrue();
    }

    [Fact]
    public void DateFormat_ShouldHaveFourValues()
    {
        Enum.GetValues<DateFormat>().Length.Should().Be(4);
    }

    [Fact]
    public void DateFormat_ShouldContainAllTypes()
    {
        Enum.IsDefined(typeof(DateFormat), DateFormat.IsoDate).Should().BeTrue();
        Enum.IsDefined(typeof(DateFormat), DateFormat.IsoDateTime).Should().BeTrue();
        Enum.IsDefined(typeof(DateFormat), DateFormat.ReverseDate).Should().BeTrue();
        Enum.IsDefined(typeof(DateFormat), DateFormat.ShortDate).Should().BeTrue();
    }

    [Fact]
    public void VersionFormat_ShouldHaveThreeValues()
    {
        Enum.GetValues<VersionFormat>().Length.Should().Be(3);
    }

    [Fact]
    public void VersionFormat_ShouldContainAllTypes()
    {
        Enum.IsDefined(typeof(VersionFormat), VersionFormat.Semantic).Should().BeTrue();
        Enum.IsDefined(typeof(VersionFormat), VersionFormat.Sequential).Should().BeTrue();
        Enum.IsDefined(typeof(VersionFormat), VersionFormat.Simple).Should().BeTrue();
    }
}
