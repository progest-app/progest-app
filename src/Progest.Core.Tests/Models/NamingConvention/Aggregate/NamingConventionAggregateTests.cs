using FluentAssertions;
using Xunit;
using Progest.Core.Extensions;
using ConventionType = Progest.Core.Models.NamingConvention.ConventionType;
using PrefixType = Progest.Core.Models.NamingConvention.PrefixType;
using SuffixType = Progest.Core.Models.NamingConvention.SuffixType;
using PrefixRule = Progest.Core.Models.NamingConvention.PrefixRule;
using SuffixRule = Progest.Core.Models.NamingConvention.SuffixRule;
using SequentialNumberingRule = Progest.Core.Models.NamingConvention.SequentialNumberingRule;


namespace Progest.Core.Tests.Models.NamingConvention.Aggregate;

public class NamingConventionAggregateTests
{
    [Fact]
    public void Constructor_WithName_ShouldCreateConvention()
    {
        // Arrange & Act
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test Convention");

        // Assert
        convention.Name.Should().Be("Test Convention");
        convention.Type.Should().Be(ConventionType.None);
    }

    [Fact]
    public void Constructor_WithAllParameters_ShouldCreateConvention()
    {
        // Arrange & Act
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test Convention",
            ConventionType.SnakeCase,
            new PrefixRule(PrefixType.Fixed, "test_"),
            new SuffixRule(SuffixType.Fixed, "_v1")
        );

        // Assert
        convention.Name.Should().Be("Test Convention");
        convention.Type.Should().Be(ConventionType.SnakeCase);
        convention.Prefix.Should().NotBeNull();
        convention.Suffix.Should().NotBeNull();
    }

    [Fact]
    public void Apply_WithNoConversionOrAffixes_ShouldReturnInput()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test");
        var input = "MyFileName";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("MyFileName");
    }

    [Fact]
    public void Apply_WithSnakeCase_ShouldConvert()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test", ConventionType.SnakeCase);
        var input = "MyFileName";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("my_file_name");
    }

    [Fact]
    public void Apply_WithCamelCase_ShouldConvert()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test", ConventionType.CamelCase);
        var input = "MyFileName";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("myFileName");
    }

    [Fact]
    public void Apply_WithPascalCase_ShouldConvert()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test", ConventionType.PascalCase);
        var input = "my_file_name";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("MyFileName");
    }

    [Fact]
    public void Apply_WithKebabCase_ShouldConvert()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test", ConventionType.KebabCase);
        var input = "MyFileName";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("my-file-name");
    }

    [Fact]
    public void Apply_WithTitleCase_ShouldConvert()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention("Test", ConventionType.TitleCase);
        var input = "myFileName";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("My File Name");
    }

    [Fact]
    public void Apply_WithPrefix_ShouldAddPrefix()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test",
            ConventionType.None,
            new PrefixRule(PrefixType.Fixed, "prefix_")
        );
        var input = "filename";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("prefix_filename");
    }

    [Fact]
    public void Apply_WithSuffix_ShouldAddSuffix()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test",
            ConventionType.None,
            null,
            new SuffixRule(SuffixType.Fixed, "_suffix")
        );
        var input = "filename";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("filename_suffix");
    }

    [Fact]
    public void Apply_WithPrefixSuffixAndConversion_ShouldApplyAll()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test",
            ConventionType.SnakeCase,
            new PrefixRule(PrefixType.Fixed, "date_"),
            new SuffixRule(SuffixType.Fixed, "_v1")
        );
        var input = "MyFileName";

        // Act
        var result = convention.Apply(input);

        // Assert
        result.Should().Be("date_my_file_name_v1");
    }

    [Fact]
    public void ApplyBatch_WithSequentialNumbering_ShouldApplyCorrectly()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test",
            ConventionType.SnakeCase,
            new PrefixRule(PrefixType.Fixed, "file_"),
            null,
            new SequentialNumberingRule(startNumber: 1, digitCount: 4, separator: "_")
        );
        var input = "MyFileName";

        // Act
        var result1 = convention.ApplyBatch(input, 0);
        var result2 = convention.ApplyBatch(input, 1);
        var result3 = convention.ApplyBatch(input, 2);

        // Assert
        result1.Should().Be("file_0001_my_file_name");
        result2.Should().Be("file_0002_my_file_name");
        result3.Should().Be("file_0003_my_file_name");
    }

    [Fact]
    public void ApplyBatch_WithAllComponents_ShouldApplyAll()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test",
            ConventionType.KebabCase,
            new PrefixRule(PrefixType.Fixed, "doc-"),
            new SuffixRule(SuffixType.Fixed, "-final"),
            new SequentialNumberingRule(startNumber: 0, digitCount: 2, separator: "-")
        );
        var input = "MyDocument";

        // Act
        var result = convention.ApplyBatch(input, 5);

        // Assert
        result.Should().Be("doc-05-my-document-final");
    }

    [Fact]
    public void ApplyBatch_WithoutSequentialNumbering_ShouldWorkLikeApply()
    {
        // Arrange
        var convention = new Progest.Core.Models.NamingConvention.NamingConvention(
            "Test",
            ConventionType.SnakeCase,
            new PrefixRule(PrefixType.Fixed, "test_"),
            new SuffixRule(SuffixType.Fixed, "_end")
        );
        var input = "MyFile";

        // Act
        var result = convention.ApplyBatch(input, 42);

        // Assert
        result.Should().Be("test_my_file_end");
    }
}
