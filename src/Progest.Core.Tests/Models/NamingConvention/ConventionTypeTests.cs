using FluentAssertions;
using Xunit;
using Progest.Core.Models.NamingConvention;

namespace Progest.Core.Tests.Models.NamingConvention;

public class ConventionTypeTests
{
    [Fact]
    public void ConventionType_ShouldHaveCorrectValues()
    {
        // Arrange & Act & Assert
        Assert.Equal(6, Enum.GetValues<ConventionType>().Length);
    }

    [Fact]
    public void ConventionType_ShouldContainAllTypes()
    {
        // Arrange & Act & Assert
        Assert.True(Enum.IsDefined(typeof(ConventionType), ConventionType.SnakeCase));
        Assert.True(Enum.IsDefined(typeof(ConventionType), ConventionType.CamelCase));
        Assert.True(Enum.IsDefined(typeof(ConventionType), ConventionType.PascalCase));
        Assert.True(Enum.IsDefined(typeof(ConventionType), ConventionType.KebabCase));
        Assert.True(Enum.IsDefined(typeof(ConventionType), ConventionType.TitleCase));
        Assert.True(Enum.IsDefined(typeof(ConventionType), ConventionType.None));
    }
}
